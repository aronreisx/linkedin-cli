use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{debug, info};

use crate::browser::transport::InPageTransport;
use crate::domain::models::{ContactInfo, InteractionRule};
use crate::domain::traits::ContactInfoFetcher;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;
use crate::infra::scraper::ChromiumProfileScraper;

pub struct ChromiumContactInfoFetcher {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumContactInfoFetcher {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }
}

#[async_trait]
impl ContactInfoFetcher for ChromiumContactInfoFetcher {
    async fn get_contact_info(&self, handle_or_url: &str, raw: bool) -> Result<(ContactInfo, Option<Value>), AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "contact", "Fetching contact info for: {public_id}");
        human_jitter(&self.rule).await;

        let voyager_url = format!(
            "https://www.linkedin.com/voyager/api/identity/profiles/{public_id}/profileContactInfo"
        );

        let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &voyager_url, &[], None, 15_000).await?;

        if ok && status == 200 {
            let json_val: Value = serde_json::from_str(&body).map_err(AppError::Json)?;

            let email = json_val.get("emailAddress").and_then(|e| e.as_str()).map(|s| s.to_string());
            let mut emails = Vec::new();
            if let Some(ref e) = email {
                emails.push(e.clone());
            }

            let mut phone_numbers = Vec::new();
            if let Some(phones) = json_val.get("phoneNumbers").and_then(|p| p.as_array()) {
                for p in phones {
                    if let Some(num) = p.get("number").and_then(|n| n.as_str()) {
                        phone_numbers.push(num.to_string());
                    }
                }
            }

            let mut websites = Vec::new();
            if let Some(sites) = json_val.get("websites").and_then(|w| w.as_array()) {
                for w in sites {
                    if let Some(url) = w.get("url").and_then(|u| u.as_str()) {
                        websites.push(url.to_string());
                    }
                }
            }

            let mut twitter_handles = Vec::new();
            if let Some(twitters) = json_val.get("twitterHandles").and_then(|t| t.as_array()) {
                for t in twitters {
                    if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                        twitter_handles.push(name.to_string());
                    }
                }
            }

            let contact_info = ContactInfo {
                public_identifier: public_id,
                email,
                emails,
                phone_numbers,
                websites,
                twitter_handles,
            };

            let raw_data = if raw { Some(json_val) } else { None };
            return Ok((contact_info, raw_data));
        }

        debug!(target: "contact", "Voyager contactInfo API returned status {status}, falling back to overlay modal navigation...");
        let overlay_url = format!("https://www.linkedin.com/in/{public_id}/overlay/contact-info/");
        self.page
            .goto(&overlay_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: overlay_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let js_extract = r#"
            (() => {
                const emails = Array.from(document.querySelectorAll('a[href^="mailto:"]'))
                    .map(a => a.innerText.trim())
                    .filter(Boolean);
                const phones = Array.from(document.querySelectorAll('ul li .t-14.t-black--light'))
                    .map(el => el.innerText.trim())
                    .filter(t => /^[+\d\s().-]{7,}$/.test(t));
                const websites = Array.from(document.querySelectorAll('.pv-contact-info__contact-link[href^="http"]'))
                    .map(a => a.href)
                    .filter(Boolean);

                return {
                    email: emails[0] || null,
                    emails: emails,
                    phone_numbers: phones,
                    websites: websites,
                    twitter_handles: []
                };
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::ExtractionFailed {
                target: overlay_url.clone(),
                reason: e.to_string(),
            })?;

        let val = eval_res
            .into_value::<Value>()
            .map_err(|e| AppError::ExtractionFailed {
                target: overlay_url.clone(),
                reason: e.to_string(),
            })?;

        let email = val["email"].as_str().map(|s| s.to_string());
        let emails = val["emails"].as_array().map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }).unwrap_or_default();
        let phone_numbers = val["phone_numbers"].as_array().map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }).unwrap_or_default();
        let websites = val["websites"].as_array().map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }).unwrap_or_default();

        let contact_info = ContactInfo {
            public_identifier: public_id,
            email,
            emails,
            phone_numbers,
            websites,
            twitter_handles: vec![],
        };

        let raw_data = if raw { Some(val) } else { None };
        Ok((contact_info, raw_data))
    }
}
