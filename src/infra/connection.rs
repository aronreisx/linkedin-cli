use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::domain::models::{ConnectionState, InteractionRule, Invitation, StatusResult};
use crate::domain::traits::{ConnectionManager, InvitationManager};
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;
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

        // UI Inspection
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

    async fn send_connection_request(
        &self,
        handle_or_url: &str,
        note: Option<&str>,
    ) -> Result<StatusResult, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Sending connection request to {public_id}");

        if let Some(n) = note {
            if n.len() > 300 {
                return Err(AppError::InvalidInput(
                    "Connection note exceeds maximum length of 300 characters".to_string(),
                ));
            }
        }

        // Ensure authenticated session is active before profile navigation
        let current_url = self.page.url().await.ok().flatten().unwrap_or_default();
        let target_in = format!("/in/{public_id}");
        let already_on_profile = current_url.contains(&target_in);

        if !already_on_profile {
            if !current_url.contains("/feed") {
                debug!(target: "connection", "Activating authenticated session via feed...");
                let _ = self.page.goto("https://www.linkedin.com/feed/").await;
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
            }

            // Navigate to profile
            let profile_url = format!("https://www.linkedin.com/in/{public_id}/");
            self.page
                .goto(&profile_url)
                .await
                .map_err(|e| AppError::NavigationFailed {
                    url: profile_url.clone(),
                    reason: e.to_string(),
                })?;
        }

        crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        human_jitter(&self.rule).await;

        let note_json = serde_json::to_string(&note).map_err(AppError::Json)?;

        let js_send_routine = format!(
            r#"
            (async () => {{
                const noteText = {note_json};
                const publicId = "{public_id}";
                const sleep = ms => new Promise(r => setTimeout(r, ms));

                // Wait for page to finish loading
                for (let i = 0; i < 20; i++) {{
                    if (document.readyState === 'complete') break;
                    await sleep(200);
                }}
                await sleep(500);

                // Check for unauthenticated redirect or authwall
                const currentHref = window.location.href;
                if (currentHref.includes('/authwall') || currentHref.includes('/uas/login') || currentHref.includes('/login') || currentHref.includes('/signup')) {{
                    return {{
                        status: "UNAUTHENTICATED",
                        reason: "Redirected to authentication page: " + currentHref
                    }};
                }}

                if (document.querySelector('a[href*="/login"], a[href*="/uas/login"]') && (document.querySelector('form.join-form, form[action*="login"]') || document.querySelector('.authwall-join-form'))) {{
                    return {{
                        status: "UNAUTHENTICATED",
                        reason: "Session is not logged in (authwall detected)"
                    }};
                }}

                // 1. Locate the profile top-card section anchored by the main h1 (member's name)
                let topCard = null;
                for (let i = 0; i < 30; i++) {{
                    const h1 = document.querySelector('main h1, .scaffold-layout__main h1, h1');
                    if (h1) {{
                        let curr = h1.parentElement;
                        while (curr && curr !== document.body && curr.tagName !== 'MAIN') {{
                            if (curr.tagName === 'SECTION' || curr.classList.contains('artdeco-card') || curr.classList.contains('pv-top-card') || (curr.getAttribute('data-view-name') || '').includes('profile-card')) {{
                                topCard = curr;
                                break;
                            }}
                            curr = curr.parentElement;
                        }}
                        if (!topCard) {{
                            topCard = h1.closest('main') || document.body;
                        }}
                        break;
                    }}
                    await sleep(200);
                }}

                if (!topCard) {{
                    topCard = document.querySelector('main') || document.body;
                }}

                // Isolate the member's profile action buttons container
                const actionsContainer = document.querySelector('.pvs-profile-actions, [data-view-name*="profile-action"], .pv-top-card-v2-ctas') || topCard;
                const topCardButtons = Array.from(actionsContainer.querySelectorAll(
                    'button, a[role="button"], a[componentkey*="ConnectButton"], a[href*="custom-invite"], a.artdeco-button'
                )).filter(b => {{
                    const rect = b.getBoundingClientRect();
                    return rect.width > 0 && rect.height > 0;
                }});

                // Check if already pending
                const isPending = topCardButtons.some(b => {{
                    const text = (b.innerText || '').trim().toLowerCase();
                    const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                    return text === 'pending' || aria.includes('pending') || aria.includes('invitation sent');
                }});
                if (isPending) {{
                    return {{ status: "ALREADY_PENDING" }};
                }}

                // Check if 1st degree connected
                const isFirstDegree = Array.from(document.querySelectorAll('span, div')).some(el => {{
                    return (el.innerText || '').trim() === '1st' && el.classList.contains('dist-value');
                }});
                if (isFirstDegree) {{
                    return {{ status: "ALREADY_CONNECTED" }};
                }}

                // 2. Look for Direct Connect action (button OR link)
                const getConnectBtn = () => {{
                    const allCandidates = Array.from(document.querySelectorAll(
                        'a[href*="/preload/custom-invite/?vanityName=' + publicId + '"], ' +
                        'a[href*="custom-invite"][href*="' + publicId + '"], ' +
                        'a[componentkey*="ConnectButtonstate:invitation"], ' +
                        'button[componentkey*="ConnectButtonstate:invitation"], ' +
                        'a[href*="custom-invite"]'
                    ));

                    // Priority A: Target vanityName specific buttons/links that are visible on the profile card (y > 100)
                    const targetMatches = allCandidates.filter(el => {{
                        const href = el.getAttribute('href') || '';
                        const aria = el.getAttribute('aria-label') || '';
                        const comp = el.getAttribute('componentkey') || '';
                        const r = el.getBoundingClientRect();
                        const isVisibleCard = r.width > 0 && r.height > 0 && r.y > 100;
                        return isVisibleCard && (
                            href.includes('/preload/custom-invite/?vanityName=' + publicId) ||
                            (href.includes('custom-invite') && href.includes(publicId)) ||
                            comp.includes('ConnectButtonstate:invitation') ||
                            (aria.toLowerCase().includes('invite') && aria.toLowerCase().includes('connect'))
                        );
                    }});

                    if (targetMatches.length > 0) {{
                        return targetMatches[0];
                    }}

                    // Priority B: Any visible button/link in actionsContainer with "connect" text
                    return topCardButtons.find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                        const r = b.getBoundingClientRect();
                        return r.width > 0 && r.height > 0 && r.y > 100 && (
                            text === 'connect' || (aria.includes('invite') && aria.includes('connect')) || aria.startsWith('connect with')
                        );
                    }}) || null;
                }};

                let connectBtn = getConnectBtn();
                let clickedDetails = null;

                if (connectBtn) {{
                    const r = connectBtn.getBoundingClientRect();
                    clickedDetails = {{
                        tag: connectBtn.tagName,
                        class: connectBtn.className,
                        href: connectBtn.getAttribute('href'),
                        aria: connectBtn.getAttribute('aria-label'),
                        text: (connectBtn.innerText || '').trim(),
                        rect: {{ x: r.x, y: r.y, w: r.width, h: r.height }}
                    }};
                    try {{ connectBtn.focus(); }} catch(e) {{}}
                    connectBtn.click();
                }} else {{
                    // 3. Look in More actions menu inside top card
                    const moreBtn = topCardButtons.find(b => {{
                        const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                        return aria.includes('more actions') || aria === 'more';
                    }}) || topCardButtons.find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        return text === 'more';
                    }}) || topCardButtons.find(b => {{
                        return (b.id || '').includes('overflow') || (b.className || '').includes('overflow');
                    }});

                    if (!moreBtn) {{
                        return {{
                            status: "NOT_FOUND",
                            reason: "Neither Connect button nor More actions button found on profile actions",
                            top_card_buttons: topCardButtons.map(b => (b.innerText || b.getAttribute('aria-label') || '').trim()).filter(Boolean)
                        }};
                    }}

                    moreBtn.click();
                    await sleep(300);

                    // Wait up to 3s for dropdown menu to appear in DOM
                    let connectOption = null;
                    let dropdownItemsSeen = [];
                    for (let i = 0; i < 15; i++) {{
                        await sleep(200);
                        const menuItems = Array.from(document.querySelectorAll(
                            '.artdeco-dropdown__content [role="button"], .artdeco-dropdown__content [role="menuitem"], .artdeco-dropdown__content li, ' +
                            '[role="menu"] [role="menuitem"], [role="menu"] [role="button"], [role="menu"] li, ' +
                            '.pvs-overflow-actions-dropdown__content [role="button"], .pvs-overflow-actions-dropdown__content span, ' +
                            'div.artdeco-dropdown__item, [data-view-name*="overflow"] [role="button"], [data-view-name*="overflow"] [role="menuitem"]'
                        ));
                        dropdownItemsSeen = menuItems.map(el => (el.innerText || el.getAttribute('aria-label') || '').trim()).filter(Boolean);
                        connectOption = menuItems.find(el => {{
                            const text = (el.innerText || '').trim().toLowerCase();
                            const aria = (el.getAttribute('aria-label') || '').trim().toLowerCase();
                            return text === 'connect' || text.includes('connect') || aria.includes('connect');
                        }});
                        if (connectOption) break;
                    }}

                    if (!connectOption) {{
                        document.body.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Escape' }}));
                        return {{
                            status: "NOT_FOUND",
                            reason: "Connect option not found in More actions dropdown",
                            more_btn: {{ aria: moreBtn.getAttribute('aria-label'), text: moreBtn.innerText, class: moreBtn.className }},
                            top_card_buttons: topCardButtons.map(b => (b.innerText || b.getAttribute('aria-label') || '').trim()).filter(Boolean),
                            dropdown_items_seen: dropdownItemsSeen
                        }};
                    }}

                    try {{ connectOption.focus(); }} catch(e) {{}}
                    connectOption.click();
                }}

                // Helpers to search across Document and all open Shadow Roots (LinkedIn puts #artdeco-modal-outlet in div.theme--light shadowRoot)
                const getAllRoots = () => {{
                    const roots = [document];
                    try {{
                        const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_ELEMENT);
                        let node;
                        while ((node = walker.nextNode())) {{
                            if (node.shadowRoot) {{
                                roots.push(node.shadowRoot);
                            }}
                        }}
                    }} catch(e) {{}}
                    return roots;
                }};

                const queryRoots = (selector) => {{
                    for (const root of getAllRoots()) {{
                        try {{
                            const el = root.querySelector(selector);
                            if (el) return el;
                        }} catch(e) {{}}
                    }}
                    return null;
                }};

                const queryAllRoots = (selector) => {{
                    let results = [];
                    for (const root of getAllRoots()) {{
                        try {{
                            results = results.concat(Array.from(root.querySelectorAll(selector)));
                        }} catch(e) {{}}
                    }}
                    return results;
                }};

                // 4. Wait for invitation modal or direct send (up to 5s)
                const findInvitationModal = () => {{
                    // 1. Artdeco modal outlet or send-invite class across all document and shadow roots
                    const outletModal = queryRoots(
                        '#artdeco-modal-outlet .artdeco-modal, ' +
                        '#artdeco-modal-outlet [role="dialog"], ' +
                        '#artdeco-modal-outlet .send-invite, ' +
                        '.send-invite, ' +
                        '.artdeco-modal.send-invite, ' +
                        '[role="dialog"].send-invite, ' +
                        'dialog[data-testid="dialog"], ' +
                        'dialog[open]'
                    );
                    if (outletModal) return outletModal;

                    // 2. Scan all dialogs and containers for invitation text across all roots
                    const dialogs = queryAllRoots('dialog, [role="dialog"], .artdeco-modal, div').filter(m => {{
                        if (m.classList.contains('vjs-modal-dialog') || m.classList.contains('vjs-error-display')) return false;
                        const t = (m.innerText || '').toLowerCase();
                        return (t.includes('add a note') && t.includes('invitation')) || t.includes('send without a note') || (t.includes('personalize') && t.includes('invitation'));
                    }});

                    if (dialogs.length > 0) {{
                        dialogs.sort((a, b) => (a.innerText || '').length - (b.innerText || '').length);
                        return dialogs[0];
                    }}

                    return null;
                }};

                let modal = null;
                for (let i = 0; i < 25; i++) {{
                    await sleep(200);
                    if (queryRoots('div[class*="ip-fuse-limit-alert__warning"], [data-testid="connection-limit-alert"]')) {{
                        return {{ status: "LIMIT_REACHED" }};
                    }}

                    modal = findInvitationModal();
                    if (modal) {{
                        const text = (modal.innerText || '').toLowerCase();
                        if (text.includes('invite') || text.includes('invitation') || text.includes('connect') || text.includes('note') || text.includes('send')) {{
                            const modalBtns = Array.from(modal.querySelectorAll('button'));
                            if (modalBtns.length > 0) break;
                        }}
                    }}

                    const pendingNow = Array.from(actionsContainer.querySelectorAll('button, a')).find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                        return text === 'pending' || aria.includes('pending') || aria.includes('invitation sent');
                    }});
                    if (pendingNow) {{
                        return {{ status: "SENT_DIRECTLY" }};
                    }}

                    // Retry clicking fresh button after 1.2s and 2.4s if modal hasn't mounted
                    if ((i === 6 || i === 12) && !modal) {{
                        const freshBtn = getConnectBtn();
                        if (freshBtn) {{
                            try {{ freshBtn.focus(); }} catch(e) {{}}
                            freshBtn.click();
                        }}
                    }}
                }}

                if (!modal) {{
                    await sleep(500);
                    const pendingNow = Array.from(actionsContainer.querySelectorAll('button, a')).find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                        return text === 'pending' || aria.includes('pending') || aria.includes('invitation sent');
                    }});
                    if (pendingNow) {{
                        return {{ status: "SENT_DIRECTLY" }};
                    }}
                    return {{
                        status: "MODAL_TIMEOUT",
                        reason: "Connect was clicked, but neither invitation modal nor Pending status appeared",
                        clicked_btn: clickedDetails,
                        all_candidates: Array.from(document.querySelectorAll('a[href*="custom-invite"]')).map(el => {{
                            const r = el.getBoundingClientRect();
                            return {{ tag: el.tagName, href: el.getAttribute('href'), y: r.y, h: r.height, w: r.width }};
                        }}),
                        top_card_buttons: Array.from(actionsContainer.querySelectorAll('button, a')).map(b => (b.innerText || b.getAttribute('aria-label') || '').trim()).filter(Boolean),
                        outlet_html: (queryRoots('#artdeco-modal-outlet') || {{}}).innerHTML || null,
                        dialog_tags: queryAllRoots('dialog, [role="dialog"], .artdeco-modal').map(d => ({{ tag: d.tagName, class: d.className, text: (d.innerText || '').slice(0, 100) }}))
                    }};
                }}

                // 5. Handle modal
                if (noteText) {{
                    const addNoteBtn = Array.from(modal.querySelectorAll('button')).find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                        return text.includes('add a note') || aria.includes('add a note');
                    }});

                    if (addNoteBtn) {{
                        addNoteBtn.click();
                        await sleep(300);
                    }}

                    let textarea = null;
                    for (let i = 0; i < 15; i++) {{
                        textarea = modal.querySelector('textarea#custom-message, textarea[name="message"], textarea');
                        if (textarea) break;
                        await sleep(200);
                    }}

                    if (textarea) {{
                        try {{ textarea.focus(); }} catch(_) {{}}
                        return {{ status: "AWAIT_NOTE_INPUT" }};
                    }} else {{
                        return {{ status: "TEXTAREA_NOT_FOUND", reason: "Could not locate note textarea in modal" }};
                    }}
                }}

                // 6. Click Send / Send without a note
                let sendBtn = null;
                for (let i = 0; i < 15; i++) {{
                    const buttons = Array.from(modal.querySelectorAll('button'));
                    sendBtn = buttons.find(b => {{
                        const text = (b.innerText || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                        if (noteText) {{
                            return text === 'send' || text === 'send invitation' || text === 'send now' || aria.includes('send invitation') || (aria.includes('send') && !aria.includes('without'));
                        }} else {{
                            return text.includes('send without a note') || text === 'send' || text === 'send now' || aria.includes('send without') || aria.includes('send now') || aria.includes('send invitation');
                        }}
                    }});
                    if (sendBtn && !sendBtn.disabled) break;
                    await sleep(200);
                }}

                if (!sendBtn) {{
                    sendBtn = Array.from(modal.querySelectorAll('button')).find(b => {{
                        const text = (b.innerText || '').toLowerCase();
                        return text.includes('send');
                    }});
                }}

                if (!sendBtn) {{
                    return {{
                        status: "SEND_BUTTON_NOT_FOUND",
                        modal_tag: modal ? modal.tagName : null,
                        modal_class: modal ? (typeof modal.className === 'string' ? modal.className : '') : null,
                        modal_text: modal ? (modal.innerText || '').slice(0, 1000) : null,
                        modal_buttons: Array.from(modal.querySelectorAll('button')).map(b => (b.innerText || b.getAttribute('aria-label') || '').trim())
                    }};
                }}

                try {{ sendBtn.focus(); }} catch(e) {{}}
                sendBtn.click();

                // Wait for modal to dismiss
                for (let i = 0; i < 20; i++) {{
                    await sleep(200);
                    const stillOpen = queryRoots('#artdeco-modal-outlet .artdeco-modal, dialog[data-testid="dialog"][open], .artdeco-modal.send-invite');
                    if (!stillOpen) break;
                }}

                return {{ status: "SENT" }};
            }})()
        "#
        );

        let mut eval_res = None;
        let mut last_err = None;
        for attempt in 1..=5 {
            match self.page.evaluate(js_send_routine.as_str()).await {
                Ok(res) => {
                    eval_res = Some(res);
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("navigated or closed")
                        || err_str.contains("Cannot find context")
                    {
                        debug!(target: "connection", "Page navigated during evaluate (attempt {attempt}/5), retrying in 1s...");
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                        last_err = Some(err_str);
                        continue;
                    }
                    return Err(AppError::Cdp(err_str));
                }
            }
        }

        let eval_res = match eval_res {
            Some(res) => res,
            None => {
                return Err(AppError::Cdp(
                    last_err.unwrap_or_else(|| "Evaluate failed after retries".to_string()),
                ));
            }
        };

        let val = eval_res
            .into_value::<Value>()
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        debug!(target: "connection", "Connection send routine result: {val:?}");

        let status_str = val["status"].as_str().unwrap_or("UNKNOWN");
        match status_str {
            "AWAIT_NOTE_INPUT" => {
                if let Some(n) = note {
                    crate::infra::typing::human_type(&self.page, n, None).await?;
                    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
                }
                // Dispatch send click on the open invitation modal
                let js_click_send = r#"
                    (async () => {
                        const sleep = ms => new Promise(r => setTimeout(r, ms));
                        const queryRoots = (sel) => {
                            const els = Array.from(document.querySelectorAll(sel));
                            document.querySelectorAll('*').forEach(el => {
                                if (el.shadowRoot) els.push(...Array.from(el.shadowRoot.querySelectorAll(sel)));
                            });
                            return els;
                        };
                        let modal = null;
                        for (let i = 0; i < 20; i++) {
                            const modals = queryRoots('#artdeco-modal-outlet .artdeco-modal, dialog[data-testid="dialog"][open], .artdeco-modal.send-invite');
                            if (modals.length > 0) { modal = modals[modals.length - 1]; break; }
                            await sleep(200);
                        }
                        if (!modal) return { status: "MODAL_NOT_FOUND" };
                        let sendBtn = null;
                        for (let i = 0; i < 15; i++) {
                            const buttons = Array.from(modal.querySelectorAll('button'));
                            sendBtn = buttons.find(b => {
                                const text = (b.innerText || '').trim().toLowerCase();
                                const aria = (b.getAttribute('aria-label') || '').trim().toLowerCase();
                                return text === 'send' || text === 'send invitation' || text === 'send now' || aria.includes('send invitation') || (aria.includes('send') && !aria.includes('without'));
                            });
                            if (sendBtn && !sendBtn.disabled) break;
                            await sleep(200);
                        }
                        if (!sendBtn) {
                            sendBtn = Array.from(modal.querySelectorAll('button')).find(b => (b.innerText || '').toLowerCase().includes('send'));
                        }
                        if (!sendBtn) return { status: "SEND_BUTTON_NOT_FOUND" };
                        try { sendBtn.focus(); } catch(e) {}
                        sendBtn.click();
                        for (let i = 0; i < 20; i++) {
                            await sleep(200);
                            const stillOpen = queryRoots('#artdeco-modal-outlet .artdeco-modal, dialog[data-testid="dialog"][open], .artdeco-modal.send-invite');
                            if (!stillOpen || stillOpen.length === 0) break;
                        }
                        return { status: "SENT" };
                    })()
                "#;
                let send_eval = self
                    .page
                    .evaluate(js_click_send)
                    .await
                    .map_err(|e| AppError::Cdp(e.to_string()))?;
                let send_val = send_eval.into_value::<Value>().unwrap_or_default();
                let send_status = send_val["status"].as_str().unwrap_or("UNKNOWN");
                if send_status != "SENT" {
                    return Err(AppError::ElementNotFound {
                        selector: format!(
                            "Connection send button after note for {public_id}: {send_val:?}"
                        ),
                    });
                }
                info!(target: "connection", "Successfully sent connection request with note to {public_id}");
                human_jitter(&self.rule).await;
                Ok(StatusResult {
                    public_identifier: public_id,
                    state: ConnectionState::Pending,
                })
            }
            "SENT" | "SENT_DIRECTLY" => {
                info!(target: "connection", "Successfully sent connection request to {public_id}");
                human_jitter(&self.rule).await;
                Ok(StatusResult {
                    public_identifier: public_id,
                    state: ConnectionState::Pending,
                })
            }
            "ALREADY_PENDING" => {
                info!(target: "connection", "Invitation is already pending for {public_id}");
                Ok(StatusResult {
                    public_identifier: public_id,
                    state: ConnectionState::Pending,
                })
            }
            "ALREADY_CONNECTED" => {
                info!(target: "connection", "{public_id} is already a 1st degree connection");
                Ok(StatusResult {
                    public_identifier: public_id,
                    state: ConnectionState::Connected,
                })
            }
            "LIMIT_REACHED" => Err(AppError::ConnectionLimit(
                "Weekly connection invitation limit reached".to_string(),
            )),
            "UNAUTHENTICATED" => {
                let reason = val["reason"].as_str().unwrap_or("Authentication required");
                Err(AppError::Authentication(format!(
                    "{reason}. Please authenticate first with 'linkedin auth login'."
                )))
            }
            _ => {
                let reason = val["reason"].as_str().unwrap_or(status_str);
                let has_follow = val["top_card_buttons"]
                    .as_array()
                    .map(|arr| {
                        arr.iter().any(|b| {
                            b.as_str()
                                .map(|s| s.eq_ignore_ascii_case("follow"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                if has_follow && reason.contains("Connect option not found") {
                    return Err(AppError::ProfileInaccessible(format!(
                        "{public_id} does not accept connection requests (member is Follow-only, reached 30k limit, or restricts connections)"
                    )));
                }

                warn!(target: "connection", "Failed to send connection request to {public_id}: {reason}");
                Err(AppError::ElementNotFound {
                    selector: format!("Connect button or Send action on {public_id}: {reason}"),
                })
            }
        }
    }
}

#[async_trait]
impl InvitationManager for ChromiumConnectionManager {
    async fn list_sent_invitations(
        &self,
        scroll: u32,
        limit: Option<usize>,
    ) -> Result<Vec<Invitation>, AppError> {
        info!(target: "invitations", "Navigating to sent invitations page...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/sent/".to_string())
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/sent/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        self.scrape_invitation_list(scroll, limit, "sent_invitations")
            .await
    }

    async fn list_received_invitations(
        &self,
        scroll: u32,
        limit: Option<usize>,
    ) -> Result<Vec<Invitation>, AppError> {
        info!(target: "invitations", "Navigating to received invitations page...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/".to_string())
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        self.scrape_invitation_list(scroll, limit, "received_invitations")
            .await
    }

    async fn withdraw_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Navigating to sent invitations to withdraw invitation for '{public_id}'...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/sent/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/sent/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_withdraw = format!(
            r#"
            (([publicId]) => {{
                return (async () => {{
                    const sleep = ms => new Promise(r => setTimeout(r, ms));
                    const targetId = (publicId || '').toLowerCase().trim();

                    let targetCard = null;
                    for (let attempt = 0; attempt < 6; attempt++) {{
                        for (let i = 0; i < 20; i++) {{
                            const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                            if (links.length > 0) break;
                            const empty = document.querySelector('.mn-invitation-manager__no-invites, [class*="empty-state"]');
                            if (empty) break;
                            await sleep(200);
                        }}

                        const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                        for (const link of links) {{
                            let path = '';
                            try {{
                                path = new URL(link.href).pathname;
                            }} catch (e) {{
                                path = link.href;
                            }}
                            const parts = path.replace(/^\/|\/$/g, '').split('/');
                            if (parts.length < 2 || parts[0] !== 'in') continue;
                            let rawId = '';
                            try {{
                                rawId = decodeURIComponent(parts[1]);
                            }} catch (e) {{
                                rawId = parts[1];
                            }}
                            const cleanId = rawId.split('?')[0].split('#')[0].trim().toLowerCase();
                            if (cleanId === targetId || link.href.toLowerCase().includes('/in/' + targetId)) {{
                                const card = link.closest('li, [role="listitem"], .invitation-card, [class*="invitation-card"]');
                                if (card) {{
                                    targetCard = card;
                                    break;
                                }}
                            }}
                        }}

                        if (targetCard) break;

                        if (attempt < 5) {{
                            window.scrollTo(0, document.body.scrollHeight);
                            const main = document.querySelector('main');
                            if (main) main.scrollTo(0, main.scrollHeight);
                            await sleep(1500);
                        }}
                    }}

                    if (!targetCard) {{
                        return {{ success: false, reason: "card_not_found" }};
                    }}

                    const candidates = Array.from(targetCard.querySelectorAll('button, a, div[role="button"], [role="button"]'));
                    const withdrawBtn = candidates.find(b => {{
                        const t = (b.innerText || b.textContent || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                        return aria.includes('withdraw') || aria.includes('retirar') ||
                               t === 'withdraw' || t === 'retirar' || t.startsWith('withdraw') || t.startsWith('retirar');
                    }});

                    if (!withdrawBtn) {{
                        return {{ success: false, reason: "withdraw_button_not_found" }};
                    }}

                    withdrawBtn.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                    await sleep(300);
                    withdrawBtn.click();

                    // Wait for confirmation dialog to appear
                    let confirmBtn = null;
                    for (let i = 0; i < 25; i++) {{
                        await sleep(200);
                        const dialogs = Array.from(document.querySelectorAll('dialog, [role="dialog"], [role="alertdialog"], .artdeco-modal'));
                        for (const dialog of dialogs) {{
                            const style = window.getComputedStyle(dialog);
                            if (style.display === 'none' || style.visibility === 'hidden') continue;

                            const buttons = Array.from(dialog.querySelectorAll('button, [role="button"]'));
                            const btn = buttons.find(b => {{
                                const t = (b.innerText || b.textContent || '').trim().toLowerCase();
                                const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                                if (t.includes('cancel') || aria.includes('cancel') || t.includes('cancelar') || aria.includes('cancelar')) {{
                                    return false;
                                }}
                                return t.includes('withdraw') || t.includes('retirar') ||
                                       aria.includes('withdraw') || aria.includes('retirar') ||
                                       b.classList.contains('artdeco-button--primary') ||
                                       b.classList.contains('artdeco-modal__confirm-dialog-btn');
                            }});
                            if (btn) {{
                                confirmBtn = btn;
                                break;
                            }}
                        }}
                        if (confirmBtn) break;
                    }}

                    if (!confirmBtn) {{
                        return {{ success: false, reason: "confirm_button_not_found" }};
                    }}

                    confirmBtn.click();

                    for (let i = 0; i < 20; i++) {{
                        await sleep(200);
                        const openDialog = document.querySelector('dialog[open], .artdeco-modal');
                        if (!openDialog) break;
                    }}

                    return {{ success: true }};
                }})();
            }})({id_json})
        "#
        );

        let res_val = self
            .page
            .evaluate(js_withdraw)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<Value>()
            .unwrap_or(Value::Null);

        let success = res_val
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if success {
            info!(target: "connection", "Successfully withdrew invitation to '{public_id}'");
            Ok(true)
        } else {
            let reason = res_val
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            warn!(target: "connection", "Failed to withdraw invitation to '{public_id}': {reason}");
            Ok(false)
        }
    }

    async fn accept_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Navigating to received invitations to accept invitation from '{public_id}'...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_accept = format!(
            r#"
            (([publicId]) => {{
                return (async () => {{
                    const sleep = ms => new Promise(r => setTimeout(r, ms));
                    const targetId = (publicId || '').toLowerCase().trim();

                    let targetCard = null;
                    for (let attempt = 0; attempt < 6; attempt++) {{
                        for (let i = 0; i < 20; i++) {{
                            const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                            if (links.length > 0) break;
                            const empty = document.querySelector('.mn-invitation-manager__no-invites, [class*="empty-state"]');
                            if (empty) break;
                            await sleep(200);
                        }}

                        const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                        for (const link of links) {{
                            let path = '';
                            try {{
                                path = new URL(link.href).pathname;
                            }} catch (e) {{
                                path = link.href;
                            }}
                            const parts = path.replace(/^\/|\/$/g, '').split('/');
                            if (parts.length < 2 || parts[0] !== 'in') continue;
                            let rawId = '';
                            try {{
                                rawId = decodeURIComponent(parts[1]);
                            }} catch (e) {{
                                rawId = parts[1];
                            }}
                            const cleanId = rawId.split('?')[0].split('#')[0].trim().toLowerCase();
                            if (cleanId === targetId || link.href.toLowerCase().includes('/in/' + targetId)) {{
                                const card = link.closest('li, [role="listitem"], .invitation-card, [class*="invitation-card"]');
                                if (card) {{
                                    targetCard = card;
                                    break;
                                }}
                            }}
                        }}

                        if (targetCard) break;

                        if (attempt < 5) {{
                            window.scrollTo(0, document.body.scrollHeight);
                            const main = document.querySelector('main');
                            if (main) main.scrollTo(0, main.scrollHeight);
                            await sleep(1500);
                        }}
                    }}

                    if (!targetCard) {{
                        return {{ success: false, reason: "card_not_found" }};
                    }}

                    const candidates = Array.from(targetCard.querySelectorAll('button, a, div[role="button"], [role="button"]'));
                    const acceptBtn = candidates.find(b => {{
                        const t = (b.innerText || b.textContent || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                        return t === 'accept' || t === 'aceitar' || t.startsWith('accept') || t.startsWith('aceitar') ||
                               aria.includes('accept') || aria.includes('aceitar');
                    }});

                    if (!acceptBtn) {{
                        return {{ success: false, reason: "accept_button_not_found" }};
                    }}

                    acceptBtn.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                    await sleep(300);
                    acceptBtn.click();
                    await sleep(1000);

                    return {{ success: true }};
                }})();
            }})({id_json})
        "#
        );

        let res_val = self
            .page
            .evaluate(js_accept)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<Value>()
            .unwrap_or(Value::Null);

        let success = res_val
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if success {
            info!(target: "connection", "Successfully accepted invitation from '{public_id}'");
            Ok(true)
        } else {
            let reason = res_val
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            warn!(target: "connection", "Failed to accept invitation from '{public_id}': {reason}");
            Ok(false)
        }
    }

    async fn ignore_invitation(&self, handle_or_url: &str) -> Result<bool, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle_or_url);
        info!(target: "connection", "Navigating to received invitations to ignore invitation from '{public_id}'...");
        self.page
            .goto("https://www.linkedin.com/mynetwork/invitation-manager/")
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: "https://www.linkedin.com/mynetwork/invitation-manager/".to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;

        let id_json = serde_json::to_string(&public_id).map_err(AppError::Json)?;
        let js_ignore = format!(
            r#"
            (([publicId]) => {{
                return (async () => {{
                    const sleep = ms => new Promise(r => setTimeout(r, ms));
                    const targetId = (publicId || '').toLowerCase().trim();

                    let targetCard = null;
                    for (let attempt = 0; attempt < 6; attempt++) {{
                        for (let i = 0; i < 20; i++) {{
                            const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                            if (links.length > 0) break;
                            const empty = document.querySelector('.mn-invitation-manager__no-invites, [class*="empty-state"]');
                            if (empty) break;
                            await sleep(200);
                        }}

                        const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                        for (const link of links) {{
                            let path = '';
                            try {{
                                path = new URL(link.href).pathname;
                            }} catch (e) {{
                                path = link.href;
                            }}
                            const parts = path.replace(/^\/|\/$/g, '').split('/');
                            if (parts.length < 2 || parts[0] !== 'in') continue;
                            let rawId = '';
                            try {{
                                rawId = decodeURIComponent(parts[1]);
                            }} catch (e) {{
                                rawId = parts[1];
                            }}
                            const cleanId = rawId.split('?')[0].split('#')[0].trim().toLowerCase();
                            if (cleanId === targetId || link.href.toLowerCase().includes('/in/' + targetId)) {{
                                const card = link.closest('li, [role="listitem"], .invitation-card, [class*="invitation-card"]');
                                if (card) {{
                                    targetCard = card;
                                    break;
                                }}
                            }}
                        }}

                        if (targetCard) break;

                        if (attempt < 5) {{
                            window.scrollTo(0, document.body.scrollHeight);
                            const main = document.querySelector('main');
                            if (main) main.scrollTo(0, main.scrollHeight);
                            await sleep(1500);
                        }}
                    }}

                    if (!targetCard) {{
                        return {{ success: false, reason: "card_not_found" }};
                    }}

                    const candidates = Array.from(targetCard.querySelectorAll('button, a, div[role="button"], [role="button"]'));
                    const ignoreBtn = candidates.find(b => {{
                        const t = (b.innerText || b.textContent || '').trim().toLowerCase();
                        const aria = (b.getAttribute('aria-label') || '').toLowerCase();
                        return t === 'ignore' || t === 'ignorar' || t === 'decline' || t === 'dismiss' ||
                               t.startsWith('ignore') || t.startsWith('ignorar') ||
                               aria.includes('ignore') || aria.includes('ignorar') || aria.includes('decline') || aria.includes('dismiss');
                    }});

                    if (!ignoreBtn) {{
                        return {{ success: false, reason: "ignore_button_not_found" }};
                    }}

                    ignoreBtn.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                    await sleep(300);
                    ignoreBtn.click();
                    await sleep(1000);

                    return {{ success: true }};
                }})();
            }})({id_json})
        "#
        );

        let res_val = self
            .page
            .evaluate(js_ignore)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<Value>()
            .unwrap_or(Value::Null);

        let success = res_val
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if success {
            info!(target: "connection", "Successfully ignored invitation from '{public_id}'");
            Ok(true)
        } else {
            let reason = res_val
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            warn!(target: "connection", "Failed to ignore invitation from '{public_id}': {reason}");
            Ok(false)
        }
    }
}

impl ChromiumConnectionManager {
    async fn scrape_invitation_list(
        &self,
        scroll: u32,
        limit: Option<usize>,
        target_label: &str,
    ) -> Result<Vec<Invitation>, AppError> {
        for _ in 0..scroll {
            let _ = self
                .page
                .evaluate("window.scrollTo(0, document.body.scrollHeight)")
                .await;
            human_jitter(&self.rule).await;
        }

        let js_extract = r#"
            (async () => {
                const sleep = ms => new Promise(r => setTimeout(r, ms));

                for (let i = 0; i < 30; i++) {
                    const links = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                    if (links.length > 0) break;
                    const empty = document.querySelector('.mn-invitation-manager__no-invites, [class*="empty-state"]');
                    if (empty) break;
                    await sleep(200);
                }

                const profileLinks = Array.from(document.querySelectorAll('a[href*="/in/"]'));
                const seen = new Set();
                const items = [];

                for (const link of profileLinks) {
                    let path = '';
                    try {
                        path = new URL(link.href).pathname;
                    } catch (e) {
                        path = link.href;
                    }
                    const parts = path.replace(/^\/|\/$/g, '').split('/');
                    if (parts.length < 2 || parts[0] !== 'in') continue;

                    let rawPublicId = '';
                    try {
                        rawPublicId = decodeURIComponent(parts[1]);
                    } catch (e) {
                        rawPublicId = parts[1];
                    }
                    const publicId = rawPublicId.split('?')[0].split('#')[0].trim();
                    if (!publicId || seen.has(publicId.toLowerCase())) continue;

                    // Walk up to card boundary
                    let card = link;
                    while (card.parentElement && card.parentElement !== document.body && card.parentElement !== document.documentElement) {
                        const parent = card.parentElement;
                        if (parent.getAttribute('role') === 'list' || parent.tagName === 'UL') break;
                        if (card.getAttribute('role') === 'listitem' || card.tagName === 'LI') break;

                        const parentUrls = Array.from(parent.querySelectorAll('a[href*="/in/"]'))
                            .map(a => {
                                const p = (new URL(a.href).pathname || '').replace(/^\/|\/$/g, '').split('/');
                                return p.length > 1 && p[0] === 'in' ? p[1] : null;
                            })
                            .filter(Boolean);
                        if (new Set(parentUrls).size > 1) break;
                        card = parent;
                    }

                    seen.add(publicId.toLowerCase());

                    // Find anchor inside card that has non-empty text (not avatar)
                    const nameLink = Array.from(card.querySelectorAll('a[href*="/in/"]')).find(a => {
                        const t = (a.innerText || a.textContent || '').trim();
                        return t.length > 0 && !t.startsWith('http');
                    }) || link;

                    let rawName = (nameLink.querySelector('span[aria-hidden="true"]')?.innerText || nameLink.innerText || nameLink.textContent || '').trim();
                    if (rawName.includes('\n')) {
                        rawName = rawName.split('\n')[0].trim();
                    }
                    rawName = rawName.replace(/\s*•\s*(1st|2nd|3rd\+?|\d+º?)\b/gi, '').trim();
                    rawName = rawName.replace(/\s*\(.*?\)$/g, '').trim();
                    const name = rawName || publicId;

                    const cardLines = (card.innerText || '')
                        .split('\n')
                        .map(l => l.trim())
                        .filter(Boolean);

                    const actionWords = new Set([
                        'accept', 'aceitar', 'ignore', 'ignorar', 'decline', 'rejeitar', 'dismiss',
                        'withdraw', 'retirar', 'cancel', 'cancelar', 'more', 'mais', 'reply', 'responder',
                        'see more', 'ver mais', 'show more', '… show more', 'connect', 'conectar',
                        'follow', 'seguir', 'message', 'mensagem'
                    ]);

                    const timeRegex = /(ago|yesterday|hour|hours|day|days|week|weeks|month|months|year|years|ontem|hora|horas|dia|dias|semana|semanas|mês|meses|ano|anos|sent|enviado)/i;

                    let time = null;
                    const timeEl = card.querySelector('time, [class*="time"], [class*="date"]');
                    if (timeEl && timeEl.innerText.trim()) {
                        time = timeEl.innerText.trim();
                    }

                    const noteEl = card.querySelector('blockquote, [class*="invitation-card__custom-message"], [class*="message-body"], [class*="note"]');
                    let explicitNote = noteEl ? noteEl.innerText.trim() : null;

                    const contentLines = cardLines.filter(line => {
                        const lower = line.toLowerCase();
                        if (actionWords.has(lower)) return false;
                        if (/^•\s*(1st|2nd|3rd\+?|\d+º?)/i.test(line)) return false;
                        if (lower.includes('mutual connection') || lower.includes('conexão em comum') || lower.includes('conexões em comum')) return false;
                        if (lower.includes('wants to connect') || lower.includes('quer conectar')) return false;
                        if (lower.includes('invited you to connect') || lower.includes('convidou você')) return false;
                        if (lower.startsWith('reply to')) return false;
                        if (/^\d+$/.test(line)) return false;
                        if (line === rawName || line === name) return false;
                        return true;
                    });

                    if (!time) {
                        time = contentLines.find(line => timeRegex.test(line)) || null;
                    }

                    const nonTimeLines = contentLines.filter(line => line !== time && !timeRegex.test(line));

                    const headline = card.querySelector('.invitation-card__subtitle, [class*="subtitle"], [class*="headline"], [class*="occupation"]')?.innerText?.trim()
                        || (nonTimeLines.length > 0 ? nonTimeLines[0] : null);

                    let note = explicitNote;
                    if (!note && nonTimeLines.length > 1) {
                        note = nonTimeLines.slice(1).find(l => l.length > 15 && l !== headline) || null;
                    }

                    items.push({
                        profile_url: `https://www.linkedin.com/in/${encodeURIComponent(publicId)}/`,
                        public_identifier: publicId,
                        name: name,
                        headline: headline,
                        sent_or_received_at: time,
                        note: note
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

        let items: Vec<Invitation> =
            eval_res
                .into_value::<Vec<Invitation>>()
                .map_err(|e| AppError::ExtractionFailed {
                    target: target_label.to_string(),
                    reason: e.to_string(),
                })?;

        if let Some(lim) = limit {
            Ok(items.into_iter().take(lim).collect())
        } else {
            Ok(items)
        }
    }
}
