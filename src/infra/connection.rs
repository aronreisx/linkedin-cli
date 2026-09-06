use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{info, warn};

use crate::browser::transport::InPageTransport;
use crate::domain::models::{ConnectionState, InteractionRule, Invitation, StatusResult};
use crate::domain::traits::{ConnectionManager, InvitationManager};
use crate::errors::AppError;
use crate::infra::jitter::{human_jitter, human_typing_delay};
use crate::infra::scraper::ChromiumProfileScraper;

pub struct ChromiumConnectionManager {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumConnectionManager {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }
}

#[async_trait]
impl ConnectionManager for ChromiumConnectionManager {
    async fn get_connection_status(&self, handle_or_url: &str) -> Result<StatusResult, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Checking connection status for {public_id}");
        human_jitter(&self.rule).await;

        // 1. Try TopCard supplementary API
        let decoration = "com.linkedin.voyager.dash.deco.identity.profile.TopCardSupplementary-120";
        let voyager_url = format!(
            "https://www.linkedin.com/voyager/api/identity/dash/profiles?decorationId={decoration}&memberIdentity={public_id}&q=memberIdentity"
        );

        let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &voyager_url, &[], None, 15_000).await?;

        if ok && status == 200 {
            if let Ok(json_val) = serde_json::from_str::<Value>(&body) {
                if let Some(included) = json_val.get("included").and_then(|i| i.as_array()) {
                    for item in included {
                        if item.get("$type").and_then(|t| t.as_str()).map(|s| s.contains("MemberRelationship")).unwrap_or(false) {
                            if let Some(dist) = item.get("distance").and_then(|d| d.as_str()) {
                                if dist == "DISTANCE_1" {
                                    return Ok(StatusResult {
                                        public_identifier: public_id,
                                        state: ConnectionState::Connected,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. UI Inspection fallback
        let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
        self.page
            .goto(&profile_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: profile_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let js_inspect = r#"
            (() => {
                const pendingBtn = document.querySelector('button[aria-label*="Pending"]');
                if (pendingBtn || document.body.innerText.includes("Pending")) {
                    const isPending = Array.from(document.querySelectorAll('button')).some(b => (b.innerText || b.getAttribute('aria-label') || '').includes('Pending'));
                    if (isPending) return "PENDING";
                }

                const connectBtn = Array.from(document.querySelectorAll('button')).find(b => {
                    const text = (b.innerText || '').trim();
                    const aria = (b.getAttribute('aria-label') || '').trim();
                    return text === 'Connect' || aria.includes('Invite') && aria.includes('connect');
                });

                if (connectBtn) return "QUALIFIED";

                // Check More button
                const moreBtn = document.querySelector('button[aria-label="More actions"], button[aria-label="More"]');
                if (moreBtn) {
                    moreBtn.click();
                    const hasConnectOption = Array.from(document.querySelectorAll('div[role="button"], li, span[role="button"]'))
                        .some(el => (el.innerText || '').trim() === 'Connect');
                    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
                    if (hasConnectOption) return "QUALIFIED";
                }

                return "QUALIFIED";
            })()
        "#;

        let res = self
            .page
            .evaluate(js_inspect)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<String>()
            .unwrap_or_else(|_| "QUALIFIED".to_string());

        let state = match res.as_str() {
            "CONNECTED" => ConnectionState::Connected,
            "PENDING" => ConnectionState::Pending,
            _ => ConnectionState::Qualified,
        };

        Ok(StatusResult {
            public_identifier: public_id,
            state,
        })
    }

    async fn send_connection_request(&self, handle_or_url: &str, note: Option<&str>) -> Result<StatusResult, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Sending connection request to {public_id}");

        if let Some(n) = note {
            if n.len() > 300 {
                return Err(AppError::InvalidInput(
                    "Connection note exceeds maximum length of 300 characters".to_string(),
                ));
            }
        }

        // Navigate to profile first
        let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
        self.page
            .goto(&profile_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: profile_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let js_click_connect = r#"
            (() => {
                // Direct connect
                const connectBtn = Array.from(document.querySelectorAll('button')).find(b => {
                    const text = (b.innerText || '').trim();
                    const aria = (b.getAttribute('aria-label') || '').trim();
                    return text === 'Connect' || (aria.includes('Invite') && aria.includes('connect'));
                });

                if (connectBtn) {
                    connectBtn.click();
                    return true;
                }

                // More -> Connect
                const moreBtn = document.querySelector('button[aria-label="More actions"], button[aria-label="More"]');
                if (moreBtn) {
                    moreBtn.click();
                    const connectOption = Array.from(document.querySelectorAll('div[role="button"], [role="menuitem"], li, span[role="button"]'))
                        .find(el => (el.innerText || '').trim().includes('Connect'));
                    if (connectOption) {
                        connectOption.click();
                        return true;
                    }
                }

                return false;
            })()
        "#;

        let clicked = self
            .page
            .evaluate(js_click_connect)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if !clicked {
            warn!(target: "connection", "No Connect button found on profile for {public_id}");
            return Ok(StatusResult {
                public_identifier: public_id,
                state: ConnectionState::Qualified,
            });
        }

        human_jitter(&self.rule).await;

        // Check if weekly limit appeared
        let limit_check = self
            .page
            .evaluate("Boolean(document.querySelector('div[class*=\"ip-fuse-limit-alert__warning\"]'))")
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if limit_check {
            return Err(AppError::ConnectionLimit("Weekly connection invitation limit reached".to_string()));
        }

        if let Some(note_text) = note {
            // Click "Add a note" button
            let js_add_note = r#"
                (() => {
                    const addNoteBtn = Array.from(document.querySelectorAll('button'))
                        .find(b => (b.innerText || b.getAttribute('aria-label') || '').includes('Add a note'));
                    if (addNoteBtn) {
                        addNoteBtn.click();
                        return true;
                    }
                    return false;
                })()
            "#;

            let add_note_clicked = self
                .page
                .evaluate(js_add_note)
                .await
                .map_err(|e| AppError::Cdp(e.to_string()))?
                .into_value::<bool>()
                .unwrap_or(false);

            if add_note_clicked {
                human_jitter(&self.rule).await;
                let text_json = serde_json::to_string(&note_text).map_err(AppError::Json)?;
                let js_type_note = format!(r#"
                    (([text]) => {{
                        const textarea = document.querySelector('textarea#custom-message, textarea[name="message"]');
                        if (textarea) {{
                            textarea.value = text;
                            textarea.dispatchEvent(new Event('input', {{ bubbles: true }}));
                            return true;
                        }}
                        return false;
                    }})({text_json})
                "#);

                self.page
                    .evaluate(js_type_note)
                    .await
                    .map_err(|e| AppError::Cdp(e.to_string()))?;

                human_typing_delay().await;
            }
        }

        // Click "Send" / "Send now" / "Send without a note"
        let js_send = r#"
            (() => {
                const sendBtn = Array.from(document.querySelectorAll('button')).find(b => {
                    const text = (b.innerText || '').trim();
                    const aria = (b.getAttribute('aria-label') || '').trim();
                    return text === 'Send' || text === 'Send now' || aria.includes('Send invitation') || aria.includes('Send without');
                });
                if (sendBtn) {
                    sendBtn.click();
                    return true;
                }
                return false;
            })()
        "#;

        self.page
            .evaluate(js_send)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        human_jitter(&self.rule).await;

        Ok(StatusResult {
            public_identifier: public_id,
            state: ConnectionState::Pending,
        })
    }
}

#[async_trait]
impl InvitationManager for ChromiumConnectionManager {
    async fn list_sent_invitations(&self, scroll: u32, limit: Option<usize>) -> Result<Vec<Invitation>, AppError> {
        info!(target: "invitations", "Navigating to sent invitations page...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/sent/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/sent/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        for _ in 0..scroll {
            let _ = self.page.evaluate("window.scrollTo(0, document.body.scrollHeight)").await;
            human_jitter(&self.rule).await;
        }

        let js_extract = r#"
            (() => {
                const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                const seen = new Set();
                const items = [];

                for (const link of links) {
                    const href = link.href;
                    const parts = href.split('/in/');
                    if (parts.length < 2) continue;
                    const publicId = parts[1].split('?')[0].split('#')[0].replace(/\/$/, '');
                    if (!publicId || seen.has(publicId)) continue;
                    seen.add(publicId);

                    let card = link;
                    while (card && card.parentElement && !card.classList.contains('invitation-card')) {
                        card = card.parentElement;
                    }

                    const name = link.innerText.trim() || publicId;
                    const headline = card ? (card.querySelector('.invitation-card__subtitle')?.innerText?.trim() || null) : null;
                    const time = card ? (card.querySelector('time')?.innerText?.trim() || null) : null;

                    items.push({
                        profile_url: href,
                        public_identifier: publicId,
                        name: name,
                        headline: headline,
                        sent_or_received_at: time,
                        note: null
                    });
                }
                return items;
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let items: Vec<Invitation> = eval_res
            .into_value::<Vec<Invitation>>()
            .map_err(|e| AppError::ExtractionFailed {
                target: "sent_invitations".to_string(),
                reason: e.to_string(),
            })?;

        if let Some(lim) = limit {
            Ok(items.into_iter().take(lim).collect())
        } else {
            Ok(items)
        }
    }

    async fn list_received_invitations(&self, scroll: u32, limit: Option<usize>) -> Result<Vec<Invitation>, AppError> {
        info!(target: "invitations", "Navigating to received invitations page...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/".to_string())
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        for _ in 0..scroll {
            let _ = self.page.evaluate("window.scrollTo(0, document.body.scrollHeight)").await;
            human_jitter(&self.rule).await;
        }

        let js_extract = r#"
            (() => {
                const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                const seen = new Set();
                const items = [];

                for (const link of links) {
                    const href = link.href;
                    const parts = href.split('/in/');
                    if (parts.length < 2) continue;
                    const publicId = parts[1].split('?')[0].split('#')[0].replace(/\/$/, '');
                    if (!publicId || seen.has(publicId)) continue;
                    seen.add(publicId);

                    let card = link;
                    while (card && card.parentElement && !card.classList.contains('invitation-card')) {
                        card = card.parentElement;
                    }

                    const name = link.innerText.trim() || publicId;
                    const headline = card ? (card.querySelector('.invitation-card__subtitle')?.innerText?.trim() || null) : null;
                    const time = card ? (card.querySelector('time')?.innerText?.trim() || null) : null;

                    items.push({
                        profile_url: href,
                        public_identifier: publicId,
                        name: name,
                        headline: headline,
                        sent_or_received_at: time,
                        note: null
                    });
                }
                return items;
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let items: Vec<Invitation> = eval_res
            .into_value::<Vec<Invitation>>()
            .map_err(|e| AppError::ExtractionFailed {
                target: "received_invitations".to_string(),
                reason: e.to_string(),
            })?;

        if let Some(lim) = limit {
            Ok(items.into_iter().take(lim).collect())
        } else {
            Ok(items)
        }
    }

    async fn withdraw_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/sent/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/sent/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_withdraw = format!(r#"
            (([publicId]) => {{
                const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                for (const link of links) {{
                    if (link.href.includes(publicId)) {{
                        let card = link;
                        while (card && card.parentElement && !card.classList.contains('invitation-card')) {{
                            card = card.parentElement;
                        }}
                        if (card) {{
                            const withdrawBtn = Array.from(card.querySelectorAll('button')).find(b => (b.innerText || '').includes('Withdraw'));
                            if (withdrawBtn) {{
                                withdrawBtn.click();
                                return true;
                            }}
                        }}
                    }}
                }}
                return false;
            }})({id_json})
        "#);

        let clicked = self
            .page
            .evaluate(js_withdraw)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if clicked {
            human_jitter(&self.rule).await;
            let _ = self.page.evaluate(r#"
                (() => {
                    const confirmBtn = Array.from(document.querySelectorAll('button.artdeco-modal__confirm-dialog-btn')).find(b => (b.innerText || '').includes('Withdraw'));
                    if (confirmBtn) confirmBtn.click();
                })()
            "#).await;
        }

        Ok(clicked)
    }

    async fn accept_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_accept = format!(r#"
            (([publicId]) => {{
                const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                for (const link of links) {{
                    if (link.href.includes(publicId)) {{
                        let card = link;
                        while (card && card.parentElement && !card.classList.contains('invitation-card')) {{
                            card = card.parentElement;
                        }}
                        if (card) {{
                            const acceptBtn = Array.from(card.querySelectorAll('button')).find(b => (b.innerText || '').includes('Accept'));
                            if (acceptBtn) {{
                                acceptBtn.click();
                                return true;
                            }}
                        }}
                    }}
                }}
                return false;
            }})({id_json})
        "#);

        let clicked = self
            .page
            .evaluate(js_accept)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        Ok(clicked)
    }

    async fn ignore_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_ignore = format!(r#"
            (([publicId]) => {{
                const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                for (const link of links) {{
                    if (link.href.includes(publicId)) {{
                        let card = link;
                        while (card && card.parentElement && !card.classList.contains('invitation-card')) {{
                            card = card.parentElement;
                        }}
                        if (card) {{
                            const ignoreBtn = Array.from(card.querySelectorAll('button')).find(b => (b.innerText || '').includes('Ignore'));
                            if (ignoreBtn) {{
                                ignoreBtn.click();
                                return true;
                            }}
                        }}
                    }}
                }}
                return false;
            }})({id_json})
        "#);

        let clicked = self
            .page
            .evaluate(js_ignore)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        Ok(clicked)
    }
}
