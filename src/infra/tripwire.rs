use std::time::{Duration, Instant};
use chromiumoxide::Page;
use tracing::{info, warn};
use crate::errors::AppError;

/// Patterns in URLs indicating an interactive security checkpoint, CAPTCHA, or challenge.
pub const CHALLENGE_URL_PATTERNS: &[&str] = &[
    "/checkpoint/",
    "/challenge/",
    "/uas/consumer-captcha/",
    "/security-check/",
    "/feed/checkpoint",
    "/uas/login/challenge",
];

/// Returns `true` if the URL contains any known security challenge pattern.
pub fn is_challenge_url(url: &str) -> bool {
    CHALLENGE_URL_PATTERNS.iter().any(|pattern| url.contains(pattern))
}

/// Inspects the current page URL and DOM state for security checkpoints, challenges, or CAPTCHA blocks.
/// Returns `Ok(Some(url))` if a challenge is detected, or `Ok(None)` if the page is clear.
pub async fn check_checkpoint_active(page: &Page) -> Result<Option<String>, AppError> {
    let current_url = page
        .url()
        .await
        .map_err(|e| AppError::Cdp(e.to_string()))?
        .unwrap_or_default();

    // 1. Check for known challenge URL signatures
    for pattern in CHALLENGE_URL_PATTERNS {
        if current_url.contains(pattern) {
            return Ok(Some(current_url));
        }
    }

    // 2. Check DOM for in-page challenge or verification dialogs
    let js_check_dom = r#"
        (() => {
            const text = (document.body ? document.body.innerText : '').toLowerCase();
            const hasChallengeText = text.includes('security verification')
                || text.includes("let's do a quick security check")
                || text.includes('quick verification')
                || text.includes('verify your identity')
                || text.includes('unusual activity');

            const hasCaptchaEl = !!document.querySelector(
                'input#captcha, iframe[src*="captcha"], div.challenge-dialog, div.captcha-container, form[action*="checkpoint"]'
            );

            return hasCaptchaEl || (hasChallengeText && (text.includes('puzzle') || text.includes('enter code') || text.includes('pin')));
        })()
    "#;

    if let Ok(eval_res) = page.evaluate(js_check_dom).await {
        if eval_res.into_value::<bool>().unwrap_or(false) {
            return Ok(Some(current_url));
        }
    }

    Ok(None)
}

/// Brings the Chromium page/tab and application window to the front on the user's desktop,
/// and fires an audible chime and macOS system notification.
pub async fn focus_browser_and_notify(page: &Page) {
    // 1. Bring tab to front in Chromium via CDP
    if let Err(e) = page.bring_to_front().await {
        tracing::debug!(target: "tripwire", "bring_to_front failed: {e}");
    }

    // 2. On macOS, activate the browser application window and trigger notification
    #[cfg(target_os = "macos")]
    {
        // Bring Chromium or Google Chrome window to the foreground
        let apple_script = r#"
            tell application "System Events"
                set procList to (every process whose name is "Chromium" or name is "Google Chrome")
                if (count of procList) > 0 then
                    set frontmost of item 1 of procList to true
                end if
            end tell
        "#;
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(apple_script)
            .spawn();

        // Display desktop notification
        let notif_script = r#"display notification "Anti-bot CAPTCHA or security checkpoint detected! Please solve it in the browser window." with title "LinkedIn Automation" sound name "Glass""#;
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(notif_script)
            .spawn();

        // Play system chime in background
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .spawn();
    }
}

/// Inspects the current page URL and DOM state for security checkpoints, challenges, or CAPTCHA blocks.
///
/// If a checkpoint is detected:
/// 1. Brings the browser tab and OS window to the front and alerts the operator.
/// 2. If interactive resolution is enabled (default), pauses and polls until the operator resolves
///    the challenge or the timeout expires.
/// 3. If unresolved or interactive resolution is disabled, logs actionable instructions and returns
///    `Err(AppError::CheckpointChallenge)`.
pub async fn assert_no_tripwire(page: &Page) -> Result<(), AppError> {
    let initial_challenge = match check_checkpoint_active(page).await? {
        Some(url) => url,
        None => return Ok(()),
    };

    // Challenge detected: focus browser window and tab, and alert the operator
    focus_browser_and_notify(page).await;

    // Check if interactive resolution is disabled (can be disabled via LINKEDIN_INTERACTIVE_CHECKPOINT=0/false)
    let interactive_enabled = std::env::var("LINKEDIN_INTERACTIVE_CHECKPOINT")
        .map(|v| !matches!(v.to_lowercase().as_str(), "0" | "false" | "no"))
        .unwrap_or(true);

    if !interactive_enabled {
        emit_checkpoint_warning(&initial_challenge);
        return Err(AppError::CheckpointChallenge(initial_challenge));
    }

    let timeout_secs = std::env::var("LINKEDIN_CHECKPOINT_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);

    warn!(
        target: "tripwire",
        "SECURITY CHECKPOINT DETECTED: {initial_challenge}. Focused browser window. Waiting up to {timeout_secs}s for manual resolution..."
    );

    eprintln!(
        "\n⚠️  [TRIPWIRE] LINKEDIN SECURITY CHECKPOINT TRIGGERED\n\
           URL: {initial_challenge}\n\
           Browser window and tab brought to front.\n\
           Waiting up to {timeout_secs}s for you to solve the checkpoint in the browser window...\n"
    );

    let start_time = Instant::now();
    let poll_interval = Duration::from_secs(2);

    while start_time.elapsed() < Duration::from_secs(timeout_secs) {
        tokio::time::sleep(poll_interval).await;

        match check_checkpoint_active(page).await {
            Ok(None) => {
                // Checkpoint is cleared!
                info!(
                    target: "tripwire",
                    "Security checkpoint resolved by operator. Resuming automation..."
                );
                eprintln!("\n✅ [TRIPWIRE] Security checkpoint resolved by operator. Resuming automation...\n");
                // Allow DOM and navigation to settle
                tokio::time::sleep(Duration::from_millis(1500)).await;
                return Ok(());
            }
            Ok(Some(_)) => {
                // Still active, continue polling
            }
            Err(e) => {
                tracing::debug!(target: "tripwire", "Error during checkpoint polling: {e}");
            }
        }
    }

    emit_checkpoint_warning(&initial_challenge);
    Err(AppError::CheckpointChallenge(initial_challenge))
}

fn emit_checkpoint_warning(url: &str) {
    warn!(
        target: "tripwire",
        "SECURITY CHECKPOINT DETECTED: {url}. Halting automation to prevent account restriction."
    );
    eprintln!(
        "\n⚠️  [TRIPWIRE] LINKEDIN SECURITY CHECKPOINT TRIGGERED\n\
           URL: {url}\n\
           Automation has been halted to prevent account restriction.\n\
           Please run 'mise run run:headed' or launch with '--remote-debugging-port=9222'\n\
           to resolve the security checkpoint interactively.\n"
    );
}
