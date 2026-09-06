use async_trait::async_trait;
use chromiumoxide::Page;
use tracing::info;

use crate::domain::models::{InteractionRule, PeopleSearchResult, ProfileSnippet};
use crate::domain::traits::PeopleSearchService;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;

pub struct ChromiumPeopleSearchService {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumPeopleSearchService {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    pub fn resolve_network_code(name: &str) -> &'static str {
        match name.to_lowercase().as_str() {
            "first" => "F",
            "second" => "S",
            "third" => "O",
            _ => "F",
        }
    }

    pub fn resolve_geo_urn(code: &str) -> String {
        match code.to_uppercase().as_str() {
            "US" => "103644278".to_string(),
            "DE" => "101282230".to_string(),
            "PT" => "100364837".to_string(),
            "UK" => "101165590".to_string(),
            "NL" => "102890719".to_string(),
            "FR" => "105015875".to_string(),
            "LU" => "104042105".to_string(),
            "CH" => "106693272".to_string(),
            "PL" => "105072130".to_string(),
            raw if raw.chars().all(|c| c.is_ascii_digit()) => raw.to_string(),
            _ => code.to_string(),
        }
    }

    pub fn resolve_job_title_id(title: &str) -> String {
        let cleaned = title.trim().to_lowercase().replace(['-', '_'], " ");
        match cleaned.as_str() {
            "any" => "-100".to_string(),
            "owner" => "1".to_string(),
            "manager" => "2".to_string(),
            "consultant" => "3".to_string(),
            "project manager" => "4".to_string(),
            "director" => "5".to_string(),
            "president" => "6".to_string(),
            "vice president" | "founder" => "7".to_string(),
            "chief executive officer" => "8".to_string(),
            "software engineer" => "9".to_string(),
            "engineer" => "10".to_string(),
            "senior software engineer" => "39".to_string(),
            "chief technology officer" => "153".to_string(),
            "technical lead" => "189".to_string(),
            "solutions architect" => "204".to_string(),
            "platform engineer" => "6483".to_string(),
            "full stack engineer" => "25201".to_string(),
            "devops engineer" => "25764".to_string(),
            "artificial intelligence engineer" => "30128".to_string(),
            "generative ai engineer" => "31823".to_string(),
            raw if raw.lstrip_neg_digits() => raw.to_string(),
            _ => title.to_string(),
        }
    }
}

trait NegDigits {
    fn lstrip_neg_digits(&self) -> bool;
}

impl NegDigits for str {
    fn lstrip_neg_digits(&self) -> bool {
        self.strip_prefix('-').unwrap_or(self).chars().all(|c| c.is_ascii_digit())
    }
}

#[async_trait]
impl PeopleSearchService for ChromiumPeopleSearchService {
    async fn search_people(
        &self,
        keywords: &str,
        networks: &[String],
        locations: &[String],
        _hiring_for: &[String],
        page: u32,
    ) -> Result<PeopleSearchResult, AppError> {
        info!(target: "search", "Searching people for: {keywords} (page {page})");
        human_jitter(&self.rule).await;

        let mut query_params = Vec::new();
        query_params.push(format!("keywords={}", urlencoding_simple(keywords)));
        query_params.push(format!("page={page}"));

        if !networks.is_empty() {
            let codes: Vec<String> = networks
                .iter()
                .map(|n| format!("\"{}\"", Self::resolve_network_code(n)))
                .collect();
            query_params.push(format!("network=[{}]", codes.join(",")));
        }

        if !locations.is_empty() {
            let geos: Vec<String> = locations
                .iter()
                .map(|l| format!("\"{}\"", Self::resolve_geo_urn(l)))
                .collect();
            query_params.push(format!("geoUrn=[{}]", geos.join(",")));
        }

        let search_url = format!(
            "https://www.linkedin.com/search/results/people/?{}",
            query_params.join("&")
        );

        self.page
            .goto(&search_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: search_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let js_extract = r#"
            (() => {
                const cards = Array.from(document.querySelectorAll('li.reusable-search__result-container, div[data-chameleon-result-urn], div.entity-result'));
                const seen = new Set();
                const results = [];

                for (const card of cards) {
                    const link = card.querySelector('a[href*="/in/"]');
                    if (!link) continue;
                    const href = link.href;
                    const parts = href.split('/in/');
                    if (parts.length < 2) continue;
                    const publicId = parts[1].split('?')[0].split('#')[0].replace(/\/$/, '');
                    if (!publicId || seen.has(publicId)) continue;
                    seen.add(publicId);

                    const nameEl = link.querySelector('span[aria-hidden="true"]') || link;
                    let rawName = nameEl.innerText.trim();
                    if (rawName.includes('\n')) {
                        rawName = rawName.split('\n')[0].trim();
                    }
                    const name = rawName || publicId;

                    const headlineEl = card.querySelector('.entity-result__primary-subtitle, div[class*="primary-subtitle"]');
                    const locationEl = card.querySelector('.entity-result__secondary-subtitle, div[class*="secondary-subtitle"]');

                    results.push({
                        public_identifier: publicId,
                        profile_url: href.split('?')[0],
                        name: name,
                        headline: headlineEl ? headlineEl.innerText.trim() : null,
                        location: locationEl ? locationEl.innerText.trim() : null
                    });
                }

                if (results.length === 0) {
                    const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                    for (const link of links) {
                        const href = link.href;
                        const parts = href.split('/in/');
                        if (parts.length < 2) continue;
                        const publicId = parts[1].split('?')[0].split('#')[0].replace(/\/$/, '');
                        if (!publicId || seen.has(publicId)) continue;
                        seen.add(publicId);

                        const nameEl = link.querySelector('span[aria-hidden="true"]') || link;
                        let rawName = nameEl.innerText.trim();
                        if (rawName.includes('\n')) {
                            rawName = rawName.split('\n')[0].trim();
                        }
                        results.push({
                            public_identifier: publicId,
                            profile_url: href.split('?')[0],
                            name: rawName || publicId,
                            headline: null,
                            location: null
                        });
                    }
                }
                return results;
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let profiles = eval_res
            .into_value::<Vec<ProfileSnippet>>()
            .unwrap_or_default();

        Ok(PeopleSearchResult {
            query: keywords.to_string(),
            page,
            network: networks.to_vec(),
            profiles,
        })
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}
