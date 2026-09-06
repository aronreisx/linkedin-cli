use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::info;

use crate::browser::transport::InPageTransport;
use crate::domain::models::{
    Conversation, InboxFilter, InboxResult, InteractionRule, Message, Participant,
};
use crate::domain::traits::{LinkedInDriver, MessagingService};
use crate::errors::AppError;
use crate::infra::driver::ChromiumLinkedInDriver;
use crate::infra::jitter::{human_jitter, human_typing_delay};
use crate::infra::scraper::ChromiumProfileScraper;

pub struct ChromiumMessagingService {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumMessagingService {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }
}

#[async_trait]
impl MessagingService for ChromiumMessagingService {
    async fn send_direct_message(&self, handle_or_url: &str, text: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "messaging", "Sending direct message to {public_id}");

        // 1. We need the target member URN. Scrape or lookup via Voyager.
        let voyager_url = format!(
            "https://www.linkedin.com/voyager/api/identity/dash/profiles?memberIdentity={public_id}&q=memberIdentity"
        );

        let mut target_urn = None;
        if let Ok((status, ok, body)) = InPageTransport::fetch(&self.page, "GET", &voyager_url, &[], None, 15_000).await {
            if ok && status == 200 {
                if let Ok(val) = serde_json::from_str::<Value>(&body) {
                    if let Some(included) = val.get("included").and_then(|i| i.as_array()) {
                        for entity in included {
                            if entity.get("$type").and_then(|t| t.as_str()).map(|s| s.contains("identity.profile.Profile")).unwrap_or(false) {
                                if let Some(u) = entity.get("entityUrn").and_then(|u| u.as_str()) {
                                    target_urn = Some(u.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Open messaging compose page or direct profile message
        if let Some(ref urn) = target_urn {
            let encoded_urn = urlencoding_simple(urn);
            let thread_url = format!("https://www.linkedin.com/messaging/thread/new/?recipient={encoded_urn}");
            self.page
                .goto(&thread_url)
                .await
                .map_err(|e| AppError::NavigationFailed {
                    url: thread_url.clone(),
                    reason: e.to_string(),
                })?;
        } else {
            let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
            self.page
                .goto(&profile_url)
                .await
                .map_err(|e| AppError::NavigationFailed {
                    url: profile_url.clone(),
                    reason: e.to_string(),
                })?;

            human_jitter(&self.rule).await;

            let js_click_message = r#"
                (() => {
                    const msgBtn = Array.from(document.querySelectorAll('button, a')).find(el => {
                        const text = (el.innerText || '').trim();
                        const aria = (el.getAttribute('aria-label') || '').trim();
                        return text === 'Message' || aria.includes('Message');
                    });
                    if (msgBtn) {
                        msgBtn.click();
                        return true;
                    }
                    return false;
                })()
            "#;

            let clicked = self
                .page
                .evaluate(js_click_message)
                .await
                .map_err(|e| AppError::Cdp(e.to_string()))?
                .into_value::<bool>()
                .unwrap_or(false);

            if !clicked {
                return Err(AppError::ElementNotFound {
                    selector: "Message button on profile".to_string(),
                });
            }
        }

        human_jitter(&self.rule).await;

        // 3. Find message compose input and type message
        let text_json = serde_json::to_string(&text).map_err(AppError::Json)?;
        let js_type_and_send = format!(r#"
            (async ([msgText]) => {{
                const composeBox = document.querySelector('div[role="textbox"][aria-label*="message"i]')
                    || document.querySelector('div.msg-form__contenteditable')
                    || document.querySelector('div[contenteditable="true"]');

                if (!composeBox) return false;

                composeBox.focus();
                document.execCommand('insertText', false, msgText);
                composeBox.dispatchEvent(new Event('input', {{ bubbles: true }}));

                const sendBtn = document.querySelector('button.msg-form__send-button')
                    || document.querySelector('button[type="submit"].msg-form__send-button')
                    || Array.from(document.querySelectorAll('button')).find(b => (b.innerText || '').trim() === 'Send');

                if (sendBtn && !sendBtn.disabled) {{
                    sendBtn.click();
                    return true;
                }}
                return false;
            }})({text_json})
        "#);

        let sent = self
            .page
            .evaluate(js_type_and_send)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        human_typing_delay().await;
        human_jitter(&self.rule).await;

        Ok(sent)
    }

    async fn get_thread(&self, handle_or_url: &str, limit: Option<usize>) -> Result<Vec<Message>, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "messaging", "Fetching thread messages for {public_id}");
        human_jitter(&self.rule).await;

        let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
        self.page
            .goto(&profile_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: profile_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let _ = self.page.evaluate(r#"
            (() => {
                const msgBtn = Array.from(document.querySelectorAll('button, a')).find(el => {
                    const text = (el.innerText || '').trim();
                    return text === 'Message';
                });
                if (msgBtn) msgBtn.click();
            })()
        "#).await;

        human_jitter(&self.rule).await;

        let js_extract_thread = r#"
            (() => {
                const msgItems = Array.from(document.querySelectorAll('li.msg-s-message-list__event, .msg-s-event-listitem'));
                return msgItems.map(item => {
                    const sender = item.querySelector('.msg-s-message-group__name')?.innerText?.trim() || null;
                    const text = item.querySelector('.msg-s-event-listitem__body')?.innerText?.trim() || null;
                    const time = item.querySelector('time')?.innerText?.trim() || null;
                    return {
                        entity_urn: null,
                        sender_urn: null,
                        sender_name: sender,
                        text: text,
                        delivered_at: null,
                        timestamp: time
                    };
                }).filter(m => m.text);
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract_thread)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let mut messages = eval_res
            .into_value::<Vec<Message>>()
            .unwrap_or_default();

        if let Some(lim) = limit {
            if messages.len() > lim {
                messages = messages.split_off(messages.len() - lim);
            }
        }

        Ok(messages)
    }

    async fn list_inbox(&self, filter: &InboxFilter, raw: bool) -> Result<(InboxResult, Option<Value>), AppError> {
        info!(target: "messaging", "Listing inbox conversations...");
        human_jitter(&self.rule).await;

        // 1. Resolve mailbox URN
        let driver = ChromiumLinkedInDriver::new(self.page.clone(), self.rule.clone());
        let self_profile = driver.whoami().await?;
        let mailbox_urn = self_profile.urn.unwrap_or_default();

        let encoded_mailbox = urlencoding_simple(&mailbox_urn);
        let graphql_base = "https://www.linkedin.com/voyager/api/voyagerMessagingGraphQL/graphql";
        let query_id = "messengerConversations.0d5e6781bbee71c3e51c8843c6519f48";
        let url = format!("{graphql_base}?queryId={query_id}&variables=(mailboxUrn:{encoded_mailbox})");

        let custom_headers = [("accept", "application/graphql")];
        let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &url, &custom_headers, None, 20_000).await?;

        let mut conversations = Vec::new();
        let next_cursor = None;
        let mut json_val: Option<Value> = None;

        if ok && status == 200 {
            if let Ok(val) = serde_json::from_str::<Value>(&body) {
                let root = val
                    .get("data")
                    .and_then(|d| d.get("messengerConversationsBySyncToken"))
                    .or_else(|| val.get("data").and_then(|d| d.get("messengerConversationsByCategoryQuery")));

                if let Some(elements) = root.and_then(|r| r.get("elements")).and_then(|e| e.as_array()) {
                    for elem in elements {
                        let entity_urn = elem.get("entityUrn").and_then(|u| u.as_str()).unwrap_or("").to_string();
                        let read = elem.get("read").and_then(|r| r.as_bool());
                        let unread_count = elem.get("unreadCount").and_then(|u| u.as_u64()).map(|u| u as u32);
                        let last_activity_at = elem.get("lastActivityAt").and_then(|l| l.as_i64());

                        let mut participants = Vec::new();
                        if let Some(parts) = elem.get("conversationParticipants").and_then(|p| p.as_array()) {
                            for p in parts {
                                let member = p.get("participantType").and_then(|pt| pt.get("member"));
                                let name = member.and_then(|m| {
                                    let first = m.get("firstName").and_then(|f| f.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                    let last = m.get("lastName").and_then(|l| l.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                    let full = format!("{first} {last}").trim().to_string();
                                    if full.is_empty() { None } else { Some(full) }
                                });
                                let urn = p.get("hostIdentityUrn").and_then(|u| u.as_str()).map(|s| s.to_string());
                                let headline = member.and_then(|m| m.get("headline").and_then(|h| h.get("text")).and_then(|t| t.as_str())).map(|s| s.to_string());
                                let profile_url = member.and_then(|m| m.get("profileUrl").and_then(|u| u.as_str())).map(|s| s.to_string());

                                participants.push(Participant {
                                    urn,
                                    name,
                                    headline,
                                    profile_url,
                                    is_self: false,
                                });
                            }
                        }

                        let mut latest_message = None;
                        if let Some(msgs) = elem.get("messages").and_then(|m| m.get("elements")).and_then(|e| e.as_array()) {
                            if let Some(first_msg) = msgs.first() {
                                let text = first_msg.get("body").and_then(|b| b.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string());
                                let delivered_at = first_msg.get("deliveredAt").and_then(|d| d.as_i64());
                                latest_message = Some(Message {
                                    entity_urn: first_msg.get("entityUrn").and_then(|u| u.as_str()).map(|s| s.to_string()),
                                    sender_urn: first_msg.get("actor").and_then(|a| a.get("hostIdentityUrn")).and_then(|u| u.as_str()).map(|s| s.to_string()),
                                    sender_name: None,
                                    text,
                                    delivered_at,
                                    timestamp: None,
                                });
                            }
                        }

                        // Apply filters
                        if filter.unread && read.unwrap_or(true) {
                            continue;
                        }
                        if filter.read && !read.unwrap_or(false) {
                            continue;
                        }
                        if let Some(ref p_filter) = filter.participant {
                            let needle = p_filter.to_lowercase();
                            let matches = participants.iter().any(|p| {
                                p.name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                                    || p.urn.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                            });
                            if !matches {
                                continue;
                            }
                        }
                        if let Some(ref contains_filter) = filter.contains {
                            let needle = contains_filter.to_lowercase();
                            let matches = latest_message
                                .as_ref()
                                .and_then(|m| m.text.as_ref())
                                .map(|t| t.to_lowercase().contains(&needle))
                                .unwrap_or(false);
                            if !matches {
                                continue;
                            }
                        }

                        conversations.push(Conversation {
                            entity_urn,
                            conversation_url: None,
                            read,
                            unread_count,
                            participants,
                            latest_message,
                            last_activity_at,
                        });
                    }
                }

                json_val = Some(val);
            }
        } else {
            // Fallback to DOM extraction from /messaging/
            info!(target: "messaging", "GraphQL inbox returned status {status}, navigating to /messaging/...");
            self.page
                .goto("https://www.linkedin.com/messaging/")
                .await
                .map_err(|e| AppError::NavigationFailed {
                    url: "https://www.linkedin.com/messaging/".to_string(),
                    reason: e.to_string(),
                })?;

            human_jitter(&self.rule).await;

            let js_extract_inbox = r#"
                (() => {
                    const convEls = Array.from(document.querySelectorAll('.msg-conversation-listitem, .msg-conversations-container__convo-item'));
                    return convEls.map(el => {
                        const name = el.querySelector('.msg-conversation-listitem__participant-names')?.innerText?.trim() || null;
                        const snippet = el.querySelector('.msg-conversation-card__message-snippet')?.innerText?.trim() || null;
                        const time = el.querySelector('time')?.innerText?.trim() || null;
                        return {
                            entity_urn: el.getAttribute('data-conversation-id') || '',
                            conversation_url: null,
                            read: true,
                            unread_count: 0,
                            participants: name ? [{ urn: null, name: name, headline: null, profile_url: null, is_self: false }] : [],
                            latest_message: snippet ? { entity_urn: null, sender_urn: null, sender_name: name, text: snippet, delivered_at: null, timestamp: time } : null,
                            last_activity_at: null
                        };
                    });
                })()
            "#;

            let eval_res = self
                .page
                .evaluate(js_extract_inbox)
                .await
                .map_err(|e| AppError::Cdp(e.to_string()))?;

            conversations = eval_res.into_value::<Vec<Conversation>>().unwrap_or_default();
        }

        if let Some(lim) = filter.limit {
            conversations.truncate(lim);
        }

        let raw_data = if raw { json_val } else { None };
        Ok((
            InboxResult {
                conversations,
                next_cursor,
            },
            raw_data,
        ))
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}
