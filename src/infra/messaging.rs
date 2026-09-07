use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{info, warn};

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

    async fn open_compose_box(&self, handle_or_url: &str) -> Result<String, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "messaging", "Opening compose box for {public_id}");

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
                                let entity_pub_id = entity.get("publicIdentifier").and_then(|p| p.as_str()).unwrap_or("");
                                if entity_pub_id.eq_ignore_ascii_case(&public_id) {
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
            crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
        } else {
            let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
            self.page
                .goto(&profile_url)
                .await
                .map_err(|e| AppError::NavigationFailed {
                    url: profile_url.clone(),
                    reason: e.to_string(),
                })?;

            crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
            human_jitter(&self.rule).await;

            let js_click_message = r#"
                (() => {
                    const buttons = Array.from(document.querySelectorAll('button, a'));
                    const msgBtn = buttons.find(el => {
                        const text = (el.innerText || '').trim();
                        const aria = (el.getAttribute('aria-label') || '').trim();
                        return text === 'Message' || aria.includes('Message');
                    });
                    if (!msgBtn) return false;
                    msgBtn.click();
                    return true;
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
                    selector: format!("Message button on profile {public_id} (not a 1st degree connection or messaging unavailable)"),
                });
            }
        }

        human_jitter(&self.rule).await;

        // 3. Find and focus compose box in DOM
        let js_focus_compose = r#"
            (async () => {
                const sleep = (ms) => new Promise(r => setTimeout(r, ms));

                const findComposeBox = () => {
                    const selectors = [
                        'div[role="textbox"][aria-label*="message"i]',
                        'div.msg-form__contenteditable[role="textbox"]',
                        '.msg-form__message-texteditor div[role="textbox"]',
                        'div.msg-form__contenteditable',
                        'div[contenteditable="true"]'
                    ];
                    for (const s of selectors) {
                        const el = document.querySelector(s);
                        if (el) return el;
                    }
                    return null;
                };

                let composeBox = null;
                for (let i = 0; i < 30; i++) {
                    composeBox = findComposeBox();
                    if (composeBox) break;
                    await sleep(200);
                }

                if (!composeBox) return false;

                try { composeBox.focus(); } catch(_) {}
                try {
                    const sel = window.getSelection();
                    const range = document.createRange();
                    range.selectNodeContents(composeBox);
                    sel.removeAllRanges();
                    sel.addRange(range);
                } catch(_) {}

                return true;
            })()
        "#;

        let focused = self
            .page
            .evaluate(js_focus_compose)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if !focused {
            return Err(AppError::ElementNotFound {
                selector: format!("Message compose textbox for {public_id} (not accessible or requires InMail)"),
            });
        }

        Ok(public_id)
    }

    async fn click_send_and_verify(&self, public_id: &str, text: Option<&str>) -> Result<bool, AppError> {
        let text_json = serde_json::to_string(&text).map_err(AppError::Json)?;
        let js_send_and_verify = format!(r#"
            (async ([msgText]) => {{
                const sleep = (ms) => new Promise(r => setTimeout(r, ms));

                const composeBox = document.querySelector('div[role="textbox"][aria-label*="message"i]')
                    || document.querySelector('div.msg-form__contenteditable');

                const findSendButton = () => {{
                    if (composeBox) {{
                        const form = composeBox.closest('form, .msg-form, .msg-convo-wrapper, aside');
                        if (form) {{
                            const btn = form.querySelector('button.msg-form__send-button, button[type="submit"]');
                            if (btn) return btn;
                        }}
                    }}
                    return document.querySelector('button.msg-form__send-button, button[type="submit"].msg-form__send-button');
                }};

                let sendBtn = null;
                for (let i = 0; i < 30; i++) {{
                    sendBtn = findSendButton();
                    if (sendBtn && !sendBtn.disabled) break;
                    await sleep(200);
                }}

                if (!sendBtn) {{
                    return {{
                        sent: false,
                        reason: "Send button not found in message form",
                        box_text: composeBox ? composeBox.innerText : null
                    }};
                }}

                if (sendBtn.disabled) {{
                    return {{
                        sent: false,
                        reason: "Send button remained disabled",
                        box_text: composeBox ? composeBox.innerText : null
                    }};
                }}

                try {{ sendBtn.focus(); }} catch(e) {{}}
                sendBtn.click();

                // Wait up to 10s for the message or attachment to appear in the message list or for input to clear
                let delivered = false;
                for (let i = 0; i < 50; i++) {{
                    await sleep(200);
                    const listItems = Array.from(document.querySelectorAll('.msg-s-message-list__event, .msg-s-event-listitem, .msg-s-message-group, .msg-s-event-listitem__body'));
                    const hasMsg = msgText ? listItems.some(item => (item.innerText || '').includes(msgText)) : false;
                    const boxCleared = composeBox && !(composeBox.innerText || '').trim();
                    const previewGone = !document.querySelector('.msg-form__attachment-preview, div[class*="attachment-preview"]');
                    if (hasMsg || (boxCleared && previewGone)) {{
                        delivered = true;
                        break;
                    }}
                }}

                return {{
                    sent: true,
                    delivered
                }};
            }})({text_json})
        "#);

        let eval_res = self
            .page
            .evaluate(js_send_and_verify)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val = eval_res.into_value::<Value>().unwrap_or_default();
        let sent = val.get("sent").and_then(|s| s.as_bool()).unwrap_or(false);

        if !sent {
            let reason = val.get("reason").and_then(|r| r.as_str()).unwrap_or("Failed to activate Send button");
            return Err(AppError::ElementNotFound {
                selector: format!("Message send action for {public_id}: {reason}"),
            });
        }

        human_typing_delay().await;
        // Delay to allow in-flight HTTP GraphQL request to settle before browser teardown
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        human_jitter(&self.rule).await;

        Ok(true)
    }
}

#[async_trait]
impl MessagingService for ChromiumMessagingService {
    async fn send_direct_message(&self, handle_or_url: &str, text: &str) -> Result<bool, AppError> {
        let public_id = self.open_compose_box(handle_or_url).await?;

        // Human-paced typing emulation through physical key events and cognitive delays
        crate::infra::typing::human_type(&self.page, text, None).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        self.click_send_and_verify(&public_id, Some(text)).await
    }

    async fn send_direct_message_with_attachment(
        &self,
        handle_or_url: &str,
        text: Option<&str>,
        attachment: &std::path::Path,
    ) -> Result<bool, AppError> {
        let abs_path = std::fs::canonicalize(attachment).map_err(AppError::Io)?;
        if !abs_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Attachment file does not exist: {}",
                attachment.display()
            )));
        }

        let public_id = self.open_compose_box(handle_or_url).await?;

        info!(
            target: "messaging",
            "Uploading attachment {} to {public_id}",
            abs_path.display()
        );

        let is_image = match abs_path.extension().and_then(|e| e.to_str()) {
            Some(ext) => matches!(
                ext.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
            ),
            None => false,
        };

        let js_mark_input = format!(
            r#"
            ((isImage) => {{
                const composeBox = document.querySelector('div[role="textbox"][aria-label*="message"i]')
                    || document.querySelector('div.msg-form__contenteditable');
                const form = composeBox ? composeBox.closest('form, .msg-form, .msg-convo-wrapper, aside') : document;
                const inputs = Array.from((form || document).querySelectorAll('input[type="file"]'));
                if (inputs.length === 0) return false;

                let target = null;
                if (isImage) {{
                    target = inputs.find(i => {{
                        const acc = (i.getAttribute('accept') || '').toLowerCase();
                        return acc.includes('image') || acc.includes('png') || acc.includes('jpg');
                    }});
                }} else {{
                    target = inputs.find(i => {{
                        const acc = (i.getAttribute('accept') || '').toLowerCase();
                        return !acc.includes('image') && acc.length > 0;
                    }});
                }}

                if (!target) {{
                    target = inputs[0];
                }}

                target.setAttribute('data-attachment-target', 'true');
                return true;
            }})({is_image})
            "#
        );

        let marked = self
            .page
            .evaluate(js_mark_input)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if !marked {
            return Err(AppError::ElementNotFound {
                selector: format!("Attachment file input in message form for {public_id}"),
            });
        }

        let file_input = self
            .page
            .find_element("input[data-attachment-target='true']")
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
        let mut set_files = SetFileInputFilesParams::new(vec![abs_path.to_string_lossy().to_string()]);
        set_files.backend_node_id = Some(file_input.backend_node_id);
        set_files.object_id = Some(file_input.remote_object_id.clone());

        self.page
            .execute(set_files)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let js_trigger_change = r#"
            (() => {
                const input = document.querySelector('input[data-attachment-target="true"]');
                if (input) {
                    input.removeAttribute('data-attachment-target');
                    input.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
                    input.dispatchEvent(new Event('input', { bubbles: true, composed: true }));
                }
            })()
        "#;
        self.page
            .evaluate(js_trigger_change)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        // Wait for attachment preview chip to mount or confirmation modal to complete
        let js_wait_attachment = r#"
            (async () => {
                const sleep = (ms) => new Promise(r => setTimeout(r, ms));
                for (let i = 0; i < 30; i++) {
                    const confirmBtn = Array.from(document.querySelectorAll('button')).find(b => {
                        const t = (b.innerText || '').trim().toLowerCase();
                        return (t === 'done' || t === 'apply' || t === 'save' || t === 'next') && !b.disabled;
                    });
                    if (confirmBtn && confirmBtn.closest('div[role="dialog"], .artdeco-modal')) {
                        try { confirmBtn.click(); } catch(e) {}
                        await sleep(500);
                    }

                    const preview = document.querySelector(
                        '.msg-form__attachment-preview, .msg-form__preview, div[class*="attachment"], button[aria-label*="Remove"i], [data-test-attachment], .msg-form__hovercard'
                    );
                    const sendBtn = document.querySelector('button.msg-form__send-button, button[type="submit"].msg-form__send-button');
                    if (preview || (sendBtn && !sendBtn.disabled)) {
                        return true;
                    }
                    await sleep(500);
                }
                return false;
            })()
        "#;

        let ready = self
            .page
            .evaluate(js_wait_attachment)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<bool>()
            .unwrap_or(false);

        if !ready {
            info!(target: "messaging", "Warning: Attachment preview chip not detected, continuing to send attempt");
        }

        // Insert optional text
        if let Some(msg_text) = text {
            if !msg_text.is_empty() {
                // Focus compose box again just in case
                let js_refocus = r#"
                    (() => {
                        const composeBox = document.querySelector('div[role="textbox"][aria-label*="message"i]')
                            || document.querySelector('div.msg-form__contenteditable');
                        if (composeBox) {
                            try { composeBox.focus(); } catch(_) {}
                        }
                    })()
                "#;
                let _ = self.page.evaluate(js_refocus).await;

                // Human-paced typing emulation through physical key events and cognitive delays
                crate::infra::typing::human_type(&self.page, msg_text, None).await?;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        self.click_send_and_verify(&public_id, text).await
    }

    async fn get_thread(&self, handle_or_url: &str, limit: Option<usize>) -> Result<Vec<Message>, AppError> {
        let public_id = self.open_compose_box(handle_or_url).await?;
        info!(target: "messaging", "Fetching thread messages for {public_id}");

        let desired_limit = limit.unwrap_or(20);
        let js_extract_thread = format!(
            r#"
            (async ([targetLimit]) => {{
                const sleep = (ms) => new Promise(r => setTimeout(r, ms));

                const findScrollContainer = () => {{
                    const list = document.querySelector('ul.msg-s-message-list, .msg-s-message-list-container, .msg-s-scroll-container');
                    if (list) {{
                        let cur = list;
                        while (cur && cur !== document.body) {{
                            const style = window.getComputedStyle(cur);
                            if ((style.overflowY === 'auto' || style.overflowY === 'scroll') && cur.scrollHeight > cur.clientHeight) {{
                                return cur;
                            }}
                            cur = cur.parentElement;
                        }}
                    }}
                    return document.querySelector('.msg-s-message-list-container, .msg-s-scroll-container, .msg-thread, .msg-s-message-list');
                }};

                // 1. Scroll up progressively to load older messages until targetLimit is reached or no more messages load
                let container = findScrollContainer();
                let prevCount = 0;
                let staleRounds = 0;

                for (let round = 0; round < 25; round++) {{
                    const currentEvents = Array.from(document.querySelectorAll('li.msg-s-message-list__event, li.msg-s-event-listitem, li.msg-s-message-listitem'))
                        .filter(el => {{
                            const body = el.querySelector('.msg-s-event-listitem__body, p, [class*="message-bubble"]');
                            return body && (body.innerText || '').trim().length > 0;
                        }});

                    if (currentEvents.length >= targetLimit) {{
                        break;
                    }}

                    if (currentEvents.length === prevCount && currentEvents.length > 0) {{
                        staleRounds++;
                        if (staleRounds >= 3) {{
                            break;
                        }}
                    }} else {{
                        staleRounds = 0;
                        prevCount = currentEvents.length;
                    }}

                    if (!container) container = findScrollContainer();
                    if (container) {{
                        container.scrollTop = 0;
                        container.dispatchEvent(new Event('scroll', {{ bubbles: true }}));
                    }}

                    const loadMoreBtn = Array.from(document.querySelectorAll('button')).find(b => {{
                        const t = (b.innerText || '').toLowerCase();
                        return t.includes('earlier') || t.includes('more messages');
                    }});
                    if (loadMoreBtn && !loadMoreBtn.disabled) {{
                        try {{ loadMoreBtn.click(); }} catch(_) {{}}
                    }}

                    await sleep(600);
                }}

                // 2. Select top-level list items only (to prevent duplicate matching with child elements)
                let msgItems = Array.from(document.querySelectorAll('li.msg-s-message-list__event, li.msg-s-event-listitem, li.msg-s-message-listitem'));
                if (msgItems.length === 0) {{
                    msgItems = Array.from(document.querySelectorAll('.msg-s-message-group'));
                }}

                const seenUrns = new Set();
                const seenKeys = new Set();
                const uniqueMessages = [];

                for (const item of msgItems) {{
                    const bodyEl = item.querySelector('.msg-s-event-listitem__body, .msg-s-message-group__message-bubble, p, [class*="message-bubble"]');
                    const text = bodyEl ? (bodyEl.innerText || '').trim() : (item.innerText || '').trim();

                    // Extract attachments (images, files/documents, audio)
                    const attachments = [];

                    // 1. Images
                    const imgEls = Array.from(item.querySelectorAll(
                        'img.msg-s-event-listitem__image, img[class*="attachment"], img[src*="media.licdn.com"], img[data-delayed-url]'
                    ));
                    for (const img of imgEls) {{
                        const src = img.getAttribute('src') || img.getAttribute('data-delayed-url') || '';
                        if (src && !src.startsWith('data:') && !src.includes('profile-displayphoto') && !src.includes('ghost-person')) {{
                            const alt = img.getAttribute('alt') || img.getAttribute('aria-label') || null;
                            attachments.push({{
                                media_type: 'image',
                                url: src,
                                name: alt && alt.length < 120 ? alt : null,
                            }});
                        }}
                    }}

                    // 2. Document attachment buttons (e.g. uploaded PDFs/documents)
                    const docBtns = Array.from(item.querySelectorAll(
                        'button.msg-s-event-listitem__download-attachment-button, button[class*="download-attachment"], div.ui-attachment, div[class*="ui-attachment"]'
                    ));
                    for (const btn of docBtns) {{
                        const filenameEl = btn.querySelector('.ui-attachment__filename, [class*="filename"]');
                        const filesizeEl = btn.querySelector('.ui-attachment__filesize, [class*="filesize"]');
                        const name = filenameEl ? filenameEl.innerText.trim() : null;
                        const size = filesizeEl ? filesizeEl.innerText.trim() : null;
                        if (name || size) {{
                            if (!attachments.some(a => a.name === name)) {{
                                attachments.push({{
                                    media_type: 'document',
                                    name: name,
                                    size: size,
                                }});
                            }}
                        }}
                    }}

                    // 3. Document / File links (e.g. downloaded from URL or cloud documents)
                    const docLinks = Array.from(item.querySelectorAll('a'));
                    for (const doc of docLinks) {{
                        const href = doc.getAttribute('href') || '';
                        const name = (doc.innerText || doc.getAttribute('download') || doc.getAttribute('aria-label') || '').trim();
                        if (href && !href.startsWith('#') && !href.includes('/in/') && !href.includes('/company/')) {{
                            const isDoc = doc.hasAttribute('download')
                                || href.includes('dms-document')
                                || href.includes('dms/prv')
                                || href.includes('licdn.com')
                                || (typeof doc.className === 'string' && (doc.className.includes('document') || doc.className.includes('attachment')))
                                || /\.(pdf|docx?|xlsx?|pptx?|zip|rar|txt|csv)(\?|$)/i.test(href)
                                || /\.(pdf|docx?|xlsx?|pptx?|zip|rar|txt|csv)/i.test(name)
                                || name.toLowerCase() === 'download'
                                || name.toLowerCase() === 'baixar';
                            if (isDoc) {{
                                if (!attachments.some(a => a.url === href)) {{
                                    attachments.push({{
                                        media_type: 'document',
                                        url: href,
                                        name: name && name.length < 120 && name.toLowerCase() !== 'download' ? name : null,
                                    }});
                                }}
                            }}
                        }}
                    }}

                    // 4. Audio / voice notes
                    const audioEls = Array.from(item.querySelectorAll('audio, [class*="voice-note"]'));
                    for (const aud of audioEls) {{
                        const src = aud.getAttribute('src') || '';
                        if (src && !attachments.some(a => a.url === src)) {{
                            attachments.push({{
                                media_type: 'audio',
                                url: src,
                                name: 'voice_note.mp3',
                            }});
                        }}
                    }}

                    // Clean text if it solely represents attachment button content
                    let cleanText = text;
                    for (const att of attachments) {{
                        if (att.name) {{
                            cleanText = cleanText.replace(att.name, '').trim();
                        }}
                        if (att.size) {{
                            cleanText = cleanText.replace(att.size, '').trim();
                        }}
                    }}
                    cleanText = cleanText.replace(/\b(Download|Baixar)\b/gi, '').trim();
                    const finalText = cleanText.length > 0 ? cleanText : null;

                    if (!finalText && attachments.length === 0) continue;

                    const groupEl = item.closest('.msg-s-message-group') || item;
                    const senderEl = groupEl.querySelector('.msg-s-message-group__name, .msg-s-message-group__profile-link, a[href*="/in/"]');
                    let sender = senderEl ? (senderEl.innerText || '').trim() : null;
                    if (sender) {{
                        sender = sender.replace(/^View\s+/i, '').replace(/['’]s\s+profile$/i, '').trim();
                    }}

                    const timeEl = item.querySelector('time') || groupEl.querySelector('time');
                    const time = timeEl ? (timeEl.innerText || timeEl.getAttribute('datetime') || '').trim() : null;

                    const urnEl = item.querySelector('[data-event-urn]') || item;
                    const urn = urnEl.getAttribute('data-event-urn') || item.getAttribute('id') || null;

                    if (urn) {{
                        if (seenUrns.has(urn)) continue;
                        seenUrns.add(urn);
                    }} else {{
                        const key = `${{sender || ''}}:${{time || ''}}:${{finalText || ''}}`;
                        if (seenKeys.has(key)) continue;
                        seenKeys.add(key);
                    }}

                    uniqueMessages.push({{
                        entity_urn: urn,
                        sender_urn: null,
                        sender_name: sender,
                        text: finalText,
                        attachments: attachments,
                        delivered_at: null,
                        timestamp: time
                    }});
                }}

                return uniqueMessages;
            }})([{desired_limit}])
            "#
        );

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

        let query_id_first = "messengerConversations.0d5e6781bbee71c3e51c8843c6519f48";
        let query_id_paged = "messengerConversations.9501074288a12f3ae9e3c7ea243bccbf";
        let graphql_base = "https://www.linkedin.com/voyager/api/voyagerMessagingGraphQL/graphql";
        let custom_headers = [("accept", "application/graphql")];

        let mut conversations = Vec::new();
        let mut next_cursor: Option<String> = None;
        let mut json_val: Option<Value> = None;

        if let Some(ref start_cursor) = filter.cursor {
            // Case 1: Start directly from the user-provided cursor
            let encoded_cursor = urlencoding_simple(start_cursor);
            let vars = format!(
                "query:(predicateUnions:List((conversationCategoryPredicate:(category:PRIMARY_INBOX)))),count:20,mailboxUrn:{encoded_mailbox},nextCursor:{encoded_cursor}"
            );
            let url = format!("{graphql_base}?queryId={query_id_paged}&variables=({vars})");
            let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &url, &custom_headers, None, 20_000).await?;
            if ok && status == 200 {
                if let Ok(val) = serde_json::from_str::<Value>(&body) {
                    let root_paged = val
                        .get("data")
                        .and_then(|d| d.get("messengerConversationsByCategoryQuery"));
                    if let Some(elements) = root_paged.and_then(|r| r.get("elements")).and_then(|e| e.as_array()) {
                        for elem in elements {
                            let conv = parse_conversation_element(elem);
                            if matches_inbox_filter(&conv, filter) {
                                conversations.push(conv);
                            }
                        }
                    }
                    next_cursor = root_paged
                        .and_then(|r| r.get("metadata"))
                        .and_then(|m| m.get("nextCursor"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                    json_val = Some(val);
                }
            } else {
                warn!(target: "messaging", "Failed to fetch paged conversations (status {status})");
            }
        } else {
            // Case 2: Fetch initial page
            let url = format!("{graphql_base}?queryId={query_id_first}&variables=(mailboxUrn:{encoded_mailbox})");
            let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &url, &custom_headers, None, 20_000).await?;

            if ok && status == 200 {
                if let Ok(val) = serde_json::from_str::<Value>(&body) {
                    let root = val
                        .get("data")
                        .and_then(|d| d.get("messengerConversationsBySyncToken"))
                        .or_else(|| val.get("data").and_then(|d| d.get("messengerConversationsByCategoryQuery")));

                    if let Some(elements) = root.and_then(|r| r.get("elements")).and_then(|e| e.as_array()) {
                        for elem in elements {
                            let conv = parse_conversation_element(elem);
                            if matches_inbox_filter(&conv, filter) {
                                conversations.push(conv);
                            }
                        }
                        if let Some(last_elem) = elements.last() {
                            next_cursor = synthesize_next_cursor(last_elem);
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

                let dom_convs = eval_res.into_value::<Vec<Conversation>>().unwrap_or_default();
                for conv in dom_convs {
                    if matches_inbox_filter(&conv, filter) {
                        conversations.push(conv);
                    }
                }
            }
        }

        // Case 3: If --all is requested, continue paginating through history
        if filter.all {
            while let Some(ref cursor_tok) = next_cursor {
                if let Some(lim) = filter.limit {
                    if conversations.len() >= lim {
                        break;
                    }
                }

                human_jitter(&self.rule).await;

                let encoded_cursor = urlencoding_simple(cursor_tok);
                let vars = format!(
                    "query:(predicateUnions:List((conversationCategoryPredicate:(category:PRIMARY_INBOX)))),count:20,mailboxUrn:{encoded_mailbox},nextCursor:{encoded_cursor}"
                );
                let url = format!("{graphql_base}?queryId={query_id_paged}&variables=({vars})");
                let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", &url, &custom_headers, None, 20_000).await?;

                if ok && status == 200 {
                    if let Ok(val) = serde_json::from_str::<Value>(&body) {
                        let root_paged = val
                            .get("data")
                            .and_then(|d| d.get("messengerConversationsByCategoryQuery"));

                        let elements_opt = root_paged.and_then(|r| r.get("elements")).and_then(|e| e.as_array());
                        if let Some(elements) = elements_opt {
                            if elements.is_empty() {
                                next_cursor = None;
                                break;
                            }
                            for elem in elements {
                                let conv = parse_conversation_element(elem);
                                if matches_inbox_filter(&conv, filter) {
                                    conversations.push(conv);
                                }
                            }
                        } else {
                            next_cursor = None;
                            break;
                        }

                        next_cursor = root_paged
                            .and_then(|r| r.get("metadata"))
                            .and_then(|m| m.get("nextCursor"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());
                        json_val = Some(val);
                    } else {
                        break;
                    }
                } else {
                    warn!(target: "messaging", "Paginating inbox failed (status {status})");
                    break;
                }
            }
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

fn parse_conversation_element(elem: &Value) -> Conversation {
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
                attachments: Vec::new(),
                delivered_at,
                timestamp: None,
            });
        }
    }

    Conversation {
        entity_urn,
        conversation_url: None,
        read,
        unread_count,
        participants,
        latest_message,
        last_activity_at,
    }
}

fn matches_inbox_filter(conv: &Conversation, filter: &InboxFilter) -> bool {
    if filter.unread && conv.read.unwrap_or(true) {
        return false;
    }
    if filter.read && !conv.read.unwrap_or(false) {
        return false;
    }
    if let Some(ref p_filter) = filter.participant {
        let needle = p_filter.to_lowercase();
        let matches = conv.participants.iter().any(|p| {
            p.name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                || p.urn.as_deref().unwrap_or("").to_lowercase().contains(&needle)
        });
        if !matches {
            return false;
        }
    }
    if let Some(ref s_filter) = filter.sender {
        let needle = s_filter.to_lowercase();
        let matches = conv.latest_message.as_ref().map(|m| {
            m.sender_name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                || m.sender_urn.as_deref().unwrap_or("").to_lowercase().contains(&needle)
        }).unwrap_or(false);
        if !matches {
            return false;
        }
    }
    if let Some(ref contains_filter) = filter.contains {
        let needle = contains_filter.to_lowercase();
        let matches = conv.latest_message
            .as_ref()
            .and_then(|m| m.text.as_ref())
            .map(|t| t.to_lowercase().contains(&needle))
            .unwrap_or(false);
        if !matches {
            return false;
        }
    }
    true
}

fn synthesize_next_cursor(elem: &Value) -> Option<String> {
    use base64::prelude::*;
    let last_activity_at = elem.get("lastActivityAt")?.as_i64()?;
    let conversation_id = elem
        .get("backendUrn")
        .and_then(|u| u.as_str())
        .map(|s| s.rsplit(':').next().unwrap_or(s))
        .or_else(|| elem.get("entityUrn").and_then(|u| u.as_str()))?;

    let raw_cursor = format!("DESCENDING&{last_activity_at}&{conversation_id}");
    Some(BASE64_STANDARD.encode(raw_cursor.as_bytes()))
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
