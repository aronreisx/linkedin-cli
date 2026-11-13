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
    async fn get_contact_info(
        &self,
        handle_or_url: &str,
        raw: bool,
    ) -> Result<(ContactInfo, Option<Value>), AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "contact", "Fetching contact info for: {public_id}");
        human_jitter(&self.rule).await;

        let voyager_url = format!(
            "https://www.linkedin.com/voyager/api/identity/profiles/{public_id}/profileContactInfo"
        );

        let (status, ok, body) =
            InPageTransport::fetch(&self.page, "GET", &voyager_url, &[], None, 15_000).await?;

        if ok && status == 200 {
            let json_val: Value = serde_json::from_str(&body).map_err(AppError::Json)?;

            let email = json_val
                .get("emailAddress")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string());
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

            let birthday = json_val.get("birthDateOn").map(|b| {
                let month = b.get("month").and_then(|m| m.as_u64());
                let day = b.get("day").and_then(|d| d.as_u64());
                match (month, day) {
                    (Some(m), Some(d)) => format!("{m}/{d}"),
                    _ => b.to_string(),
                }
            });
            let connected = json_val
                .get("connectedAt")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());

            let contact_info = ContactInfo {
                public_identifier: public_id,
                email,
                emails,
                phone_numbers,
                websites,
                twitter_handles,
                birthday,
                connected,
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

        crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
        human_jitter(&self.rule).await;

        let js_extract = r#"
            (async () => {
                const sleep = (ms) => new Promise(r => setTimeout(r, ms));

                const isContactDialog = (el) => {
                    if (!el) return false;
                    const h = el.querySelector('h1, h2, h3, header, [data-testid="dialog-header"]');
                    const text = (h ? h.innerText : el.innerText || '').toLowerCase();
                    return text.includes('contact info');
                };

                const findContactModal = () => {
                    return document.querySelector('[data-sdui-screen*="ContactDetails"]')
                        || Array.from(document.querySelectorAll('dialog, [data-testid="dialog"], .artdeco-modal, [role="dialog"]')).find(isContactDialog);
                };

                let modal = findContactModal();

                // If contact modal is not already open, find and click "Contact info" link/button
                if (!modal) {
                    const findBtn = () => document.querySelector('#top-card-text-details-contact-info')
                        || document.querySelector('a[href*="overlay/contact-info"]')
                        || Array.from(document.querySelectorAll('a, button')).find(el => {
                            const t = (el.innerText || '').trim().toLowerCase();
                            return t === 'contact info' || t.startsWith('contact info');
                        });

                    let btn = findBtn();
                    if (!btn) {
                        for (let i = 0; i < 30; i++) {
                            await sleep(200);
                            btn = findBtn();
                            if (btn) break;
                        }
                    }

                    if (btn) {
                        btn.click();
                        for (let i = 0; i < 40; i++) {
                            await sleep(200);
                            modal = findContactModal();
                            if (modal) {
                                const text = (modal.innerText || '').toLowerCase();
                                if (modal.querySelector('a') || text.includes('profile') || text.includes('website') || text.includes('email') || text.includes('phone') || text.length > 60) {
                                    break;
                                }
                            }
                        }
                    }
                }

                // If modal was already found, ensure contents are hydrated
                if (modal) {
                    for (let i = 0; i < 30; i++) {
                        const text = (modal.innerText || '').toLowerCase();
                        if (modal.querySelector('a') || text.includes('profile') || text.includes('website') || text.includes('email') || text.includes('phone') || text.length > 60) {
                            break;
                        }
                        await sleep(200);
                    }
                }

                if (!modal) {
                    return {
                        email: null,
                        emails: [],
                        phone_numbers: [],
                        websites: [],
                        twitter_handles: [],
                    };
                }

                const emails = [];
                const websites = [];
                const twitterHandles = [];
                const phones = [];

                // 2. Parse all anchor tags in modal
                const anchors = Array.from(modal.querySelectorAll('a'));
                for (const a of anchors) {
                    let href = a.href || '';
                    if (href.includes('/redir/redirect') && href.includes('url=')) {
                        try {
                            const u = new URL(href);
                            href = decodeURIComponent(u.searchParams.get('url') || href);
                        } catch (_) {}
                    }

                    const text = (a.innerText || a.textContent || '').trim();

                    if (href.startsWith('mailto:')) {
                        const em = href.replace(/^mailto:/i, '').split('?')[0].trim();
                        if (em.includes('@')) emails.push(em);
                    } else if (text.includes('@') && !text.includes(' ') && text.includes('.')) {
                        emails.push(text);
                    }

                    if (href.includes('twitter.com/') || href.includes('x.com/')) {
                        try {
                            const handle = new URL(href).pathname.replace(/^\//, '').split('/')[0];
                            if (handle) twitterHandles.push(handle);
                        } catch (_) {
                            if (text) twitterHandles.push(text);
                        }
                    }

                    if (href.startsWith('http') && !href.includes('linkedin.com/in/') && !href.includes('licdn.com')) {
                        try {
                            const u = new URL(href);
                            if (!u.hostname.endsWith('linkedin.com')) {
                                websites.push(href);
                            }
                        } catch (_) {}
                    } else if (text.includes('.') && !text.includes(' ') && !text.includes('linkedin.com') && (text.includes('.tech') || text.includes('.com') || text.includes('.org') || text.includes('.io') || text.includes('.net') || text.includes('.co'))) {
                        websites.push(text.startsWith('http') ? text : `https://${text}`);
                    }
                }

                let birthday = null;
                let connected = null;

                // 3. Text parsing for SDUI dialog structures
                const modalText = modal.innerText || '';
                const lines = modalText.split('\n').map(l => l.trim()).filter(Boolean);
                for (let i = 0; i < lines.length; i++) {
                    const line = lines[i].toLowerCase();
                    if (line === 'website' || line === 'websites') {
                        if (i + 1 < lines.length) {
                            const nextLine = lines[i + 1];
                            const domain = nextLine.split('(')[0].trim();
                            if (domain.includes('.') && !domain.includes(' ') && !domain.includes('linkedin.com')) {
                                websites.push(domain.startsWith('http') ? domain : `https://${domain}`);
                            }
                        }
                    }
                    if (line === 'email' || line === 'emails') {
                        if (i + 1 < lines.length) {
                            const nextLine = lines[i + 1].trim();
                            if (nextLine.includes('@')) emails.push(nextLine);
                        }
                    }
                    if (line === 'phone' || line === 'phone number') {
                        if (i + 1 < lines.length) {
                            const nextLine = lines[i + 1].trim();
                            if (/^[+\d\s().-]{7,}$/.test(nextLine)) phones.push(nextLine);
                        }
                    }
                    if (line === 'twitter' || line === 'x') {
                        if (i + 1 < lines.length) {
                            twitterHandles.push(lines[i + 1].trim());
                        }
                    }
                    if (line === 'birthday') {
                        if (i + 1 < lines.length) {
                            birthday = lines[i + 1].trim();
                        }
                    } else if (line.startsWith('birthday:')) {
                        birthday = lines[i].substring('birthday:'.length).trim();
                    }
                    if (line === 'connected') {
                        if (i + 1 < lines.length) {
                            connected = lines[i + 1].trim();
                        }
                    } else if (line.startsWith('connected:')) {
                        connected = lines[i].substring('connected:'.length).trim();
                    }
                }

                if (!birthday) {
                    const bdayEl = modal.querySelector('.pv-contact-info__contact-type.ci-birthday, [data-testid="birthday"]');
                    if (bdayEl) {
                        const bdayText = bdayEl.querySelector('.pv-contact-info__contact-item, span')?.innerText || '';
                        if (bdayText.trim()) birthday = bdayText.trim();
                    }
                }
                if (!connected) {
                    const connEl = modal.querySelector('.pv-contact-info__contact-type.ci-connected, [data-testid="connected"]');
                    if (connEl) {
                        const connText = connEl.querySelector('.pv-contact-info__contact-item, span')?.innerText || '';
                        if (connText.trim()) connected = connText.trim();
                    }
                }

                return {
                    email: emails[0] || null,
                    emails: Array.from(new Set(emails)),
                    phone_numbers: Array.from(new Set(phones)),
                    websites: Array.from(new Set(websites)),
                    twitter_handles: Array.from(new Set(twitterHandles)),
                    birthday: birthday || null,
                    connected: connected || null,
                    _debug_modal_tag: modal.tagName,
                    _debug_modal_text: modal.innerText.slice(0, 1500),
                    _debug_anchors: Array.from(modal.querySelectorAll('a')).map(a => ({ text: a.innerText, href: a.href })),
                };
            })()
        "#;

        let eval_res =
            self.page
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
        let emails = val["emails"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let phone_numbers = val["phone_numbers"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let websites = val["websites"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let twitter_handles = val["twitter_handles"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let birthday = val["birthday"].as_str().map(|s| s.to_string());
        let connected = val["connected"].as_str().map(|s| s.to_string());

        let contact_info = ContactInfo {
            public_identifier: public_id,
            email,
            emails,
            phone_numbers,
            websites,
            twitter_handles,
            birthday,
            connected,
        };

        let raw_data = if raw { Some(val) } else { None };
        Ok((contact_info, raw_data))
    }
}
