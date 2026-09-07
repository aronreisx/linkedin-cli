use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{debug, info};

use crate::browser::transport::InPageTransport;
use crate::domain::models::{Date, DateRange, Education, InteractionRule, Position, Profile};
use crate::domain::traits::ProfileScraper;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;

pub struct ChromiumProfileScraper {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumProfileScraper {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    pub fn extract_public_id(handle_or_url: &str) -> String {
        let trimmed = handle_or_url.trim().trim_end_matches('/');
        let raw_id = if let Some(pos) = trimmed.find("/in/") {
            let after = &trimmed[pos + 4..];
            let clean = after.split('?').next().unwrap_or(after).split('#').next().unwrap_or(after);
            clean.split('/').next().unwrap_or(clean)
        } else if trimmed.starts_with("http") {
            trimmed.split('/').next_back().unwrap_or(trimmed)
        } else {
            trimmed
        };
        percent_encoding::percent_decode_str(raw_id)
            .decode_utf8_lossy()
            .to_string()
    }
}

#[async_trait]
impl ProfileScraper for ChromiumProfileScraper {
    async fn scrape_profile(&self, handle_or_url: &str, raw: bool) -> Result<(Profile, Option<Value>), AppError> {
        let public_id = Self::extract_public_id(handle_or_url);
        let profile_url = if handle_or_url.starts_with("http") {
            handle_or_url.to_string()
        } else {
            format!("https://www.linkedin.com/in/{public_id}")
        };

        info!(target: "scraper", "Scraping profile for handle: {public_id}");
        human_jitter(&self.rule).await;

        let decoration = "com.linkedin.voyager.dash.deco.identity.profile.FullProfileWithEntities-91";
        let voyager_url = format!(
            "https://www.linkedin.com/voyager/api/identity/dash/profiles?decorationId={decoration}&memberIdentity={public_id}&q=memberIdentity"
        );

        let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &voyager_url, &[], None, 20_000).await?;

        if ok && status == 200 {
            let json_val: Value = serde_json::from_str(&body).map_err(AppError::Json)?;

            let parsed_profile = parse_voyager_profile(&json_val, &public_id, &profile_url);
            let raw_data = if raw { Some(json_val) } else { None };

            return Ok((parsed_profile, raw_data));
        }

        if status == 404 {
            return Err(AppError::ProfileInaccessible(public_id));
        }

        debug!(target: "scraper", "Voyager API scrape failed (status {status}), falling back to DOM extraction...");
        self.page
            .goto(&profile_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: profile_url.clone(),
                reason: e.to_string(),
            })?;

        crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
        human_jitter(&self.rule).await;

        let js_extract = r#"
            (() => {
                const name = document.querySelector('h1')?.innerText?.trim() || '';
                const headline = document.querySelector('.text-body-medium')?.innerText?.trim() || null;
                const location = document.querySelector('.text-body-small.inline.t-black--light.break-words')?.innerText?.trim() || null;
                return {
                    name,
                    headline,
                    location
                };
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::ExtractionFailed {
                target: profile_url.clone(),
                reason: e.to_string(),
            })?;

        let val = eval_res
            .into_value::<Value>()
            .map_err(|e| AppError::ExtractionFailed {
                target: profile_url.clone(),
                reason: e.to_string(),
            })?;

        let name = val["name"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            return Err(AppError::ProfileInaccessible(public_id));
        }

        let headline = val["headline"].as_str().map(|s| s.to_string());
        let location = val["location"].as_str().map(|s| s.to_string());

        let profile = Profile {
            urn_or_id: public_id.clone(),
            public_identifier: Some(public_id),
            urn: None,
            name,
            first_name: None,
            last_name: None,
            headline,
            location,
            profile_url,
            connection_degree: None,
            positions: vec![],
            educations: vec![],
        };

        let raw_data = if raw { Some(val) } else { None };
        Ok((profile, raw_data))
    }
}

fn parse_voyager_profile(json_val: &Value, public_id: &str, profile_url: &str) -> Profile {
    let mut urn = None;
    let mut name = String::new();
    let mut first_name = None;
    let mut last_name = None;
    let mut headline = None;
    let mut location = None;
    let mut target_geo_urn = None;
    let mut connection_degree = None;
    let mut positions = Vec::new();
    let mut educations = Vec::new();

    // Identify target member URN from collection elements if present
    let target_urn = json_val
        .get("data")
        .and_then(|d| d.get("*elements"))
        .and_then(|e| e.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut geo_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    if let Some(included) = json_val.get("included").and_then(|i| i.as_array()) {
        // First pass: collect Geo lookup table
        for item in included {
            let entity_type = item.get("$type").and_then(|t| t.as_str()).unwrap_or("");
            if entity_type.contains("common.Geo") {
                if let (Some(u), Some(loc_name)) = (
                    item.get("entityUrn").and_then(|u| u.as_str()),
                    item.get("defaultLocalizedName").and_then(|n| n.as_str()),
                ) {
                    geo_map.insert(u.to_string(), loc_name.to_string());
                }
            }
        }

        // Second pass: extract profile and associated entities
        for item in included {
            let entity_type = item.get("$type").and_then(|t| t.as_str()).unwrap_or("");

            if entity_type.contains("identity.profile.Profile") && !entity_type.contains("Position") && !entity_type.contains("Education") {
                let item_urn = item.get("entityUrn").and_then(|u| u.as_str());
                let item_pub_id = item.get("publicIdentifier").and_then(|p| p.as_str());

                // Disambiguate target profile from current viewer/other included profiles
                let is_target = match (&target_urn, item_urn, item_pub_id) {
                    (Some(expected), Some(actual), _) if expected == actual => true,
                    (_, _, Some(actual_pub)) if actual_pub.eq_ignore_ascii_case(public_id) => true,
                    (None, _, _) if urn.is_none() => true,
                    _ => false,
                };

                if is_target {
                    if let Some(u) = item_urn {
                        urn = Some(u.to_string());
                    }
                    first_name = item.get("firstName").and_then(|f| f.as_str()).map(|s| s.to_string());
                    last_name = item.get("lastName").and_then(|l| l.as_str()).map(|s| s.to_string());
                    name = format!(
                        "{} {}",
                        first_name.as_deref().unwrap_or(""),
                        last_name.as_deref().unwrap_or("")
                    )
                    .trim()
                    .to_string();

                    headline = item.get("headline").and_then(|h| h.as_str()).map(|s| s.to_string());
                    location = item.get("locationName").and_then(|l| l.as_str()).map(|s| s.to_string());
                    target_geo_urn = item
                        .get("geoLocation")
                        .and_then(|g| g.get("geoUrn").or_else(|| g.get("*geo")))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string());
                }
            } else if entity_type.contains("identity.profile.Position") {
                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
                let company_name = item.get("companyName").and_then(|c| c.as_str()).unwrap_or("").to_string();
                let company_urn = item.get("companyUrn").and_then(|c| c.as_str()).map(|s| s.to_string());
                let loc = item.get("locationName").and_then(|l| l.as_str()).map(|s| s.to_string());
                let desc = item.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());

                let date_range = item.get("dateRange").map(parse_date_range);

                if !title.is_empty() || !company_name.is_empty() {
                    positions.push(Position {
                        title,
                        company_name,
                        company_urn,
                        location: loc,
                        date_range,
                        description: desc,
                    });
                }
            } else if entity_type.contains("identity.profile.Education") {
                let school_name = item.get("schoolName").and_then(|s| s.as_str()).unwrap_or("").to_string();
                let degree_name = item.get("degreeName").and_then(|d| d.as_str()).map(|s| s.to_string());
                let field_of_study = item.get("fieldOfStudy").and_then(|f| f.as_str()).map(|s| s.to_string());
                let date_range = item.get("dateRange").map(parse_date_range);

                if !school_name.is_empty() {
                    educations.push(Education {
                        school_name,
                        degree_name,
                        field_of_study,
                        date_range,
                    });
                }
            } else if entity_type.contains("MemberRelationship") {
                if let Some(dist) = item.get("distance").and_then(|d| d.as_str()) {
                    connection_degree = match dist {
                        "DISTANCE_1" => Some(1),
                        "DISTANCE_2" => Some(2),
                        "DISTANCE_3" => Some(3),
                        _ => None,
                    };
                }
            }
        }
    }

    // Resolve location from Geo table if locationName is null
    if location.is_none() {
        if let Some(ref g_urn) = target_geo_urn {
            location = geo_map.get(g_urn).cloned();
        }
    }

    if name.is_empty() {
        name = public_id.to_string();
    }

    Profile {
        urn_or_id: public_id.to_string(),
        public_identifier: Some(public_id.to_string()),
        urn,
        name,
        first_name,
        last_name,
        headline,
        location,
        profile_url: profile_url.to_string(),
        connection_degree,
        positions,
        educations,
    }
}

fn parse_date_range(v: &Value) -> DateRange {
    let start = v.get("start").map(|s| Date {
        year: s.get("year").and_then(|y| y.as_i64()).map(|y| y as i32),
        month: s.get("month").and_then(|m| m.as_u64()).map(|m| m as u32),
    });
    let end = v.get("end").map(|e| Date {
        year: e.get("year").and_then(|y| y.as_i64()).map(|y| y as i32),
        month: e.get("month").and_then(|m| m.as_u64()).map(|m| m as u32),
    });
    DateRange { start, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_public_id_variants() {
        assert_eq!(ChromiumProfileScraper::extract_public_id("john-doe"), "john-doe");
        assert_eq!(
            ChromiumProfileScraper::extract_public_id("https://www.linkedin.com/in/franciscojmguedes/en"),
            "franciscojmguedes"
        );
        assert_eq!(
            ChromiumProfileScraper::extract_public_id("https://www.linkedin.com/in/in%C3%AAsloureiro21/en/"),
            "inêsloureiro21"
        );
        assert_eq!(
            ChromiumProfileScraper::extract_public_id("https://www.linkedin.com/in/someone/?miniProfileUrn=xyz#details"),
            "someone"
        );
        assert_eq!(
            ChromiumProfileScraper::extract_public_id("https://www.linkedin.com/in/user-name//"),
            "user-name"
        );
    }
}
