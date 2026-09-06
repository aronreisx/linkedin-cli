use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::browser::transport::InPageTransport;
use crate::domain::models::{AuthStatus, InteractionRule, SelfProfile};
use crate::domain::page_state::{classify_page_url, PageState};
use crate::domain::traits::LinkedInDriver;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;

pub struct ChromiumLinkedInDriver {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumLinkedInDriver {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    pub async fn page_url(&self) -> Result<String, AppError> {
        self.page
            .url()
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .ok_or_else(|| AppError::NavigationFailed {
                url: "unknown".to_string(),
                reason: "Page URL returned None".to_string(),
            })
    }
}

#[async_trait]
impl LinkedInDriver for ChromiumLinkedInDriver {
    async fn check_auth_status(&self) -> Result<AuthStatus, AppError> {
        human_jitter(&self.rule).await;
        let mut current_url = self.page_url().await.unwrap_or_default();
        let mut state = classify_page_url(&current_url);

        debug!(target: "driver", "Checking auth status at initial URL: {current_url} (state: {:?})", state);

        // If not already on feed or profile, navigate to feed to verify active session
        if state != PageState::Feed && state != PageState::Profile && state != PageState::Checkpoint {
            info!(target: "driver", "Navigating to LinkedIn feed to verify active session...");
            self.navigate_to("https://www.linkedin.com/feed/").await?;
        }

        // Wait up to 5 seconds for redirects or DOM navigation to stabilize
        let wait_start = std::time::Instant::now();
        while wait_start.elapsed().as_secs() < 5 {
            current_url = self.page_url().await.unwrap_or_default();
            state = classify_page_url(&current_url);

            if state == PageState::Checkpoint {
                return Ok(AuthStatus::ChallengeCheckpoint { url: current_url });
            }
            if state == PageState::Login || state == PageState::Authwall {
                return Ok(AuthStatus::Unauthenticated);
            }

            let has_nav = self
                .page
                .evaluate("Boolean(document.querySelector('.global-nav') || document.querySelector('#global-nav'))")
                .await
                .map_err(|e| AppError::Cdp(e.to_string()))?
                .into_value::<bool>()
                .unwrap_or(false);

            if has_nav || state == PageState::Feed || state == PageState::Profile {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
        }

        current_url = self.page_url().await.unwrap_or_default();
        state = classify_page_url(&current_url);

        match state {
            PageState::Checkpoint => {
                return Ok(AuthStatus::ChallengeCheckpoint { url: current_url });
            }
            PageState::Login | PageState::Authwall => {
                return Ok(AuthStatus::Unauthenticated);
            }
            _ => {}
        }

        // Verify session validity by attempting whoami via Voyager API
        match self.whoami().await {
            Ok(self_profile) => {
                let account_name = self_profile.full_name.clone();
                Ok(AuthStatus::Authenticated {
                    account_name,
                    self_profile: Some(self_profile),
                })
            }
            Err(e) => {
                debug!(target: "driver", "whoami() lookup during status check had error: {e}");
                let has_nav = self
                    .page
                    .evaluate("Boolean(document.querySelector('.global-nav') || document.querySelector('#global-nav'))")
                    .await
                    .map_err(|e| AppError::Cdp(e.to_string()))?
                    .into_value::<bool>()
                    .unwrap_or(false);

                if has_nav || state == PageState::Feed {
                    Ok(AuthStatus::Authenticated {
                        account_name: None,
                        self_profile: None,
                    })
                } else {
                    Ok(AuthStatus::Unauthenticated)
                }
            }
        }
    }

    async fn authenticate(&self, username: Option<&str>, password: Option<&str>) -> Result<SelfProfile, AppError> {
        human_jitter(&self.rule).await;
        let current_url = self.page_url().await.unwrap_or_default();
        let state = classify_page_url(&current_url);

        if state != PageState::Feed && state != PageState::Profile {
            // If on authwall or not on login page, navigate to login
            if state != PageState::Login {
                info!(target: "driver", "Navigating to LinkedIn login page...");
                self.navigate_to("https://www.linkedin.com/login").await?;
            }

            let u = username.ok_or_else(|| {
                AppError::Authentication("No LinkedIn username provided or found in environment (LINKEDIN_USERNAME)".to_string())
            })?;
            let p = password.ok_or_else(|| {
                AppError::Authentication("No LinkedIn password provided or found in environment (LINKEDIN_PASSWORD)".to_string())
            })?;

            info!(target: "driver", "Filling login credentials with human typing pacing for {u}...");

            let args_json = serde_json::to_string(&json!([u, p])).map_err(AppError::Json)?;
            let js_fill = format!(r#"
                (async ([username, password]) => {{
                    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
                    const randomBetween = (min, max) => Math.floor(Math.random() * (max - min + 1)) + min;

                    // 1. Dismiss cookie consent banner if present
                    const cookieBtn = document.querySelector('button[action-type="ACCEPT"]') 
                        || document.querySelector('.cookie-banner__accept-btn')
                        || document.querySelector('#artdeco-global-alert-container button')
                        || document.querySelector('button[id*="onetrust-accept"]');
                    if (cookieBtn) {{
                        try {{ cookieBtn.click(); await sleep(randomBetween(300, 600)); }} catch (_) {{}}
                    }}

                    const isVisible = (el) => {{
                        if (!el) return false;
                        const r = el.getBoundingClientRect();
                        const s = window.getComputedStyle(el);
                        return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden';
                    }};

                    // 2. Select VISIBLE inputs and button
                    const emailCandidates = Array.from(document.querySelectorAll(
                        'input[autocomplete~="username"], input[type="email"], #username, input[name="session_key"], #session_key'
                    ));
                    const userInput = emailCandidates.find(isVisible) || emailCandidates[0];

                    const passCandidates = Array.from(document.querySelectorAll(
                        'input[autocomplete~="current-password"], input[type="password"], #password, input[name="session_password"], #session_password'
                    ));
                    const passInput = passCandidates.find(isVisible) || passCandidates[0];

                    const allButtons = Array.from(document.querySelectorAll('button'));
                    const buttonCandidates = allButtons.filter(b => {{
                        const txt = (b.innerText || b.textContent || '').trim().toLowerCase();
                        return txt === 'sign in' || txt === 'entrar' || b.type === 'submit';
                    }});
                    const submitBtn = buttonCandidates.find(isVisible) || buttonCandidates[0];

                    if (!userInput || !passInput || !submitBtn) {{
                        return false;
                    }}

                    // 3. Human-like character-by-character typing simulation
                    const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set;
                    
                    const typeHuman = async (input, text) => {{
                        input.focus();
                        input.dispatchEvent(new Event('focus', {{ bubbles: true }}));
                        await sleep(randomBetween(150, 300));

                        // Clear input initially
                        if (nativeSetter) {{
                            nativeSetter.call(input, '');
                        }} else {{
                            input.value = '';
                        }}
                        input.dispatchEvent(new Event('input', {{ bubbles: true }}));

                        let currentVal = '';
                        for (const char of text) {{
                            currentVal += char;
                            if (nativeSetter) {{
                                nativeSetter.call(input, currentVal);
                            }} else {{
                                input.value = currentVal;
                            }}
                            input.dispatchEvent(new KeyboardEvent('keydown', {{ key: char, bubbles: true }}));
                            input.dispatchEvent(new KeyboardEvent('keypress', {{ key: char, bubbles: true }}));
                            input.dispatchEvent(new Event('input', {{ bubbles: true }}));
                            input.dispatchEvent(new KeyboardEvent('keyup', {{ key: char, bubbles: true }}));

                            const strokeDelay = (char === '@' || char === '.') ? randomBetween(120, 220) : randomBetween(45, 110);
                            await sleep(strokeDelay);
                        }}

                        input.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        await sleep(randomBetween(150, 300));
                        input.blur();
                        input.dispatchEvent(new Event('blur', {{ bubbles: true }}));
                    }};

                    // Type email
                    await typeHuman(userInput, username);

                    // Pause between fields like a human moving to the next field
                    await sleep(randomBetween(600, 1200));

                    // Type password
                    await typeHuman(passInput, password);

                    // Natural human pause before submitting (reviewing entries, moving mouse)
                    await sleep(randomBetween(1000, 2200));

                    // 4. Dispatch realistic mouse hover and press sequence
                    submitBtn.focus();
                    submitBtn.dispatchEvent(new MouseEvent('mouseover', {{ bubbles: true, cancelable: true, view: window }}));
                    submitBtn.dispatchEvent(new MouseEvent('mouseenter', {{ bubbles: true, cancelable: true, view: window }}));
                    submitBtn.dispatchEvent(new MouseEvent('mousemove', {{ bubbles: true, cancelable: true, view: window }}));
                    await sleep(randomBetween(80, 180));

                    submitBtn.dispatchEvent(new MouseEvent('mousedown', {{ bubbles: true, cancelable: true, view: window }}));
                    // Realistic physical button press hold time
                    await sleep(randomBetween(70, 140));
                    submitBtn.dispatchEvent(new MouseEvent('mouseup', {{ bubbles: true, cancelable: true, view: window }}));
                    submitBtn.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true, view: window }}));
                    submitBtn.click();
                    return true;
                }})({args_json})
            "#);

            // Wait up to 15 seconds for login DOM inputs to render
            let start = std::time::Instant::now();
            let mut filled = false;
            while start.elapsed().as_secs() < 15 {
                let current_url = self.page_url().await.unwrap_or_default();
                let current_st = classify_page_url(&current_url);
                if current_st == PageState::Checkpoint || current_st == PageState::Feed {
                    filled = true;
                    break;
                }

                let attempt = self
                    .page
                    .evaluate(js_fill.clone())
                    .await
                    .map_err(|e| AppError::Cdp(e.to_string()))?
                    .into_value::<bool>()
                    .unwrap_or(false);

                if attempt {
                    filled = true;
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
            }

            if !filled {
                let final_url = self.page_url().await.unwrap_or_default();
                return Err(AppError::ElementNotFound {
                    selector: format!("Visible login inputs (email/password/submit) on {final_url}"),
                });
            }

            // Wait for form submission to process and navigate away from login
            info!(target: "driver", "Credentials submitted to Sign in button. Waiting for authentication response...");
            let nav_start = std::time::Instant::now();
            let mut reached_auth_or_checkpoint = false;

            while nav_start.elapsed().as_secs() < 30 {
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                let current_url = self.page_url().await.unwrap_or_default();
                let current_state = classify_page_url(&current_url);

                if current_state == PageState::Feed || current_state == PageState::Profile {
                    info!(target: "driver", "Successfully authenticated! Redirected to {current_url}");
                    reached_auth_or_checkpoint = true;
                    break;
                }

                if current_state == PageState::Checkpoint {
                    info!(target: "driver", "Redirected to security checkpoint / 2FA: {current_url}");
                    reached_auth_or_checkpoint = true;
                    break;
                }

                // Check for inline error feedback on login page
                if current_state == PageState::Login {
                    let err_js = r#"
                        (() => {
                            const el = document.querySelector('.alert--error')
                                || document.querySelector('#error-for-username')
                                || document.querySelector('#error-for-password')
                                || document.querySelector('.artdeco-inline-feedback--error')
                                || document.querySelector('div[role="alert"]');
                            return el ? (el.innerText || el.textContent || '').trim() : null;
                        })()
                    "#;
                    if let Ok(val) = self.page.evaluate(err_js).await {
                        if let Ok(Some(err_msg)) = val.into_value::<Option<String>>() {
                            if !err_msg.is_empty() {
                                return Err(AppError::Authentication(format!(
                                    "LinkedIn login rejected: {err_msg}"
                                )));
                            }
                        }
                    }
                }
            }

            let current_url = self.page_url().await.unwrap_or_default();
            let current_state = classify_page_url(&current_url);

            if current_state == PageState::Checkpoint {
                warn!(
                    target: "driver",
                    "Two-factor authentication / security verification required at {current_url}.\nPlease accept the prompt on your phone or complete the verification in the browser window (waiting up to 300s)..."
                );
                let checkpoint_start = std::time::Instant::now();
                let mut cleared = false;
                while checkpoint_start.elapsed().as_secs() < 300 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let u = self.page_url().await.unwrap_or_default();
                    let st = classify_page_url(&u);
                    if st == PageState::Feed || st == PageState::Profile || (st != PageState::Checkpoint && st != PageState::Login) {
                        info!(target: "driver", "Two-factor authentication cleared! Redirected to {u}");
                        cleared = true;
                        break;
                    }
                }

                if !cleared {
                    let final_url = self.page_url().await.unwrap_or_default();
                    return Err(AppError::CheckpointChallenge(final_url));
                }
            } else if !reached_auth_or_checkpoint && current_state == PageState::Login {
                return Err(AppError::Authentication(format!(
                    "Login timed out waiting for redirect from {current_url}. Check credentials or browser window."
                )));
            }

            // Brief jitter to allow cookies to settle before Voyager query
            human_jitter(&self.rule).await;
        }

        self.whoami().await
    }

    async fn whoami(&self) -> Result<SelfProfile, AppError> {
        let voyager_url = "https://www.linkedin.com/voyager/api/identity/dash/profiles?q=memberIdentity&memberIdentity=me";
        let (status, ok, body) = InPageTransport::fetch(&self.page, "GET", voyager_url, &[], None, 15_000).await?;

        if !ok || status == 401 || status == 403 {
            return Err(AppError::Authentication(format!(
                "Failed to fetch own profile via Voyager API (HTTP {status})"
            )));
        }

        let json_val: Value = serde_json::from_str(&body).map_err(AppError::Json)?;

        let mut public_identifier = None;
        let mut urn = None;
        let mut full_name = None;

        if let Some(included) = json_val.get("included").and_then(|i| i.as_array()) {
            for entity in included {
                if entity.get("$type").and_then(|t| t.as_str()).map(|s| s.contains("Profile")).unwrap_or(false) {
                    if let Some(pid) = entity.get("publicIdentifier").and_then(|p| p.as_str()) {
                        public_identifier = Some(pid.to_string());
                    }
                    if let Some(u) = entity.get("entityUrn").and_then(|u| u.as_str()) {
                        urn = Some(u.to_string());
                    }
                    let first = entity.get("firstName").and_then(|f| f.as_str()).unwrap_or("");
                    let last = entity.get("lastName").and_then(|l| l.as_str()).unwrap_or("");
                    if !first.is_empty() || !last.is_empty() {
                        full_name = Some(format!("{first} {last}").trim().to_string());
                    }
                    break;
                }
            }
        }

        if public_identifier.is_none() {
            if let Some(elements) = json_val.get("elements").and_then(|e| e.as_array()) {
                if let Some(first) = elements.first() {
                    public_identifier = first.get("publicIdentifier").and_then(|p| p.as_str()).map(|s| s.to_string());
                    urn = first.get("entityUrn").and_then(|u| u.as_str()).map(|s| s.to_string());
                }
            }
        }

        Ok(SelfProfile {
            public_identifier,
            urn,
            full_name,
        })
    }

    async fn navigate_to(&self, url: &str) -> Result<(), AppError> {
        info!(target: "driver", "Navigating to {url}");
        self.page
            .goto(url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: url.to_string(),
                reason: e.to_string(),
            })?;
        human_jitter(&self.rule).await;
        Ok(())
    }

    async fn evaluate_js(&self, script: &str) -> Result<Value, AppError> {
        self.page
            .evaluate(script)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?
            .into_value::<Value>()
            .map_err(|e| AppError::Cdp(e.to_string()))
    }
}
