use chromiumoxide::Page;
use tracing::warn;
use crate::errors::AppError;

/// Patterns in URLs indicating an interactive security checkpoint, CAPTCHA, or challenge.
const CHALLENGE_URL_PATTERNS: &[&str] = &[
    "/checkpoint/",
    "/challenge/",
    "/uas/consumer-captcha/",
    "/security-check/",
    "/feed/checkpoint",
    "/uas/login/challenge",
];

/// Inspects the current page URL and DOM state for security checkpoints, challenges, or CAPTCHA blocks.
///
/// If a checkpoint is detected, logs actionable instructions to `stderr` and halts automation by
/// returning `Err(AppError::CheckpointChallenge)`.
pub async fn assert_no_tripwire(page: &Page) -> Result<(), AppError> {
    let current_url = page.url().await
        .map_err(|e| AppError::Cdp(e.to_string()))?
        .unwrap_or_default();

    // 1. Check for known challenge URL signatures
    for pattern in CHALLENGE_URL_PATTERNS {
        if current_url.contains(pattern) {
            emit_checkpoint_warning(&current_url);
            return Err(AppError::CheckpointChallenge(current_url));
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
            emit_checkpoint_warning(&current_url);
            return Err(AppError::CheckpointChallenge(current_url));
        }
    }

    Ok(())
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
