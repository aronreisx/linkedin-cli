use crate::errors::AppError;
use chromiumoxide::cdp::browser_protocol::input::InsertTextParams;
use chromiumoxide::Page;
use rand::Rng;
use std::time::Duration;
use tracing::debug;

/// Configuration for the Gaussian human typing engine.
#[derive(Debug, Clone)]
pub struct HumanTypingConfig {
    /// Minimum keystroke delay in milliseconds (default: 45)
    pub min_delay_ms: u64,
    /// Maximum keystroke delay in milliseconds (default: 180)
    pub max_delay_ms: u64,
    /// Probability of a typo occurring on an alphabetic character (default: 0.015 = 1.5%)
    pub typo_rate: f64,
}

impl Default for HumanTypingConfig {
    fn default() -> Self {
        Self {
            min_delay_ms: 45,
            max_delay_ms: 180,
            typo_rate: 0.015,
        }
    }
}

/// Generates a delay in milliseconds following a Gaussian (normal) bell curve,
/// centered at (min + max) / 2 with standard deviation (max - min) / 4,
/// clamped between min and max.
pub fn sample_gaussian_delay(min_ms: u64, max_ms: u64) -> u64 {
    if min_ms >= max_ms {
        return min_ms;
    }
    let mean = (min_ms + max_ms) as f64 / 2.0;
    let std_dev = (max_ms - min_ms) as f64 / 4.0;
    let mut rng = rand::thread_rng();
    // Box-Muller transform for true normal distribution
    let u1: f64 = rng.gen_range(0.00001..1.0);
    let u2: f64 = rng.gen_range(0.00001..1.0);
    let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    let val = mean + z0 * std_dev;
    val.round().clamp(min_ms as f64, max_ms as f64) as u64
}

/// Returns a realistic adjacent QWERTY keyboard character for typo simulation.
pub fn get_adjacent_qwerty_key(c: char) -> char {
    let lower = c.to_ascii_lowercase();
    let adj = match lower {
        'q' => 'w',
        'w' => 'e',
        'e' => 'r',
        'r' => 't',
        't' => 'y',
        'y' => 'u',
        'u' => 'i',
        'i' => 'o',
        'o' => 'p',
        'p' => 'o',
        'a' => 's',
        's' => 'd',
        'd' => 'f',
        'f' => 'g',
        'g' => 'h',
        'h' => 'j',
        'j' => 'k',
        'k' => 'l',
        'l' => 'k',
        'z' => 'x',
        'x' => 'c',
        'c' => 'v',
        'v' => 'b',
        'b' => 'n',
        'n' => 'm',
        'm' => 'n',
        _ => 'e',
    };
    if c.is_uppercase() {
        adj.to_ascii_uppercase()
    } else {
        adj
    }
}

/// Dispatches a key press (KeyDown followed by KeyUp) using physical CDP events.
pub async fn press_key(page: &Page, key: &str) -> Result<(), AppError> {
    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchKeyEventParams, DispatchKeyEventType,
    };

    if let Some(key_definition) = chromiumoxide::keys::get_key_definition(key) {
        let mut cmd = DispatchKeyEventParams::builder();

        let key_down_event_type = if let Some(txt) = key_definition.text {
            cmd = cmd.text(txt);
            DispatchKeyEventType::KeyDown
        } else if key_definition.key.len() == 1 {
            cmd = cmd.text(key_definition.key);
            DispatchKeyEventType::KeyDown
        } else {
            DispatchKeyEventType::RawKeyDown
        };

        cmd = cmd
            .r#type(DispatchKeyEventType::KeyDown)
            .key(key_definition.key)
            .code(key_definition.code)
            .windows_virtual_key_code(key_definition.key_code)
            .native_virtual_key_code(key_definition.key_code);

        let down_cmd = cmd
            .clone()
            .r#type(key_down_event_type)
            .build()
            .map_err(|e| AppError::Cdp(e.to_string()))?;
        page.execute(down_cmd)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let up_cmd = cmd
            .r#type(DispatchKeyEventType::KeyUp)
            .build()
            .map_err(|e| AppError::Cdp(e.to_string()))?;
        page.execute(up_cmd)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        Ok(())
    } else {
        // Fallback for characters without an explicit US keyboard key definition
        page.execute(InsertTextParams::new(key))
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;
        Ok(())
    }
}

/// Types text into the currently focused element on the page using realistic human
/// motor and cognitive timings.
///
/// Features:
/// - Emulates physical keystroke events (`DispatchKeyEvent` KeyDown/KeyUp)
/// - Normal/Gaussian distribution of keystroke delays
/// - Cognitive modifier pauses for capital letters (Shift key), numbers, punctuation, and spaces
/// - Occasional human typo injection with realistic cognitive realization and backspace correction
/// - Safe fallback to `InsertTextParams` for complex Unicode / emoji
pub async fn human_type(
    page: &Page,
    text: &str,
    config: Option<&HumanTypingConfig>,
) -> Result<(), AppError> {
    let default_cfg = HumanTypingConfig::default();
    let cfg = config.unwrap_or(&default_cfg);

    debug!(target: "typing", "Typing {} characters with Gaussian cadence...", text.chars().count());

    for ch in text.chars() {
        // 1. Simulate occasional human typo and backspace correction
        let should_typo = ch.is_ascii_alphabetic() && rand::thread_rng().gen_bool(cfg.typo_rate);
        if should_typo {
            let typo = get_adjacent_qwerty_key(ch);
            let typo_str = typo.to_string();
            if press_key(page, &typo_str).await.is_ok() {
                // Cognitive realization delay (pause before realizing mistake)
                let realization_ms = rand::thread_rng().gen_range(200..=380);
                tokio::time::sleep(Duration::from_millis(realization_ms)).await;

                // Backspace to delete erroneous character
                let _ = press_key(page, "Backspace").await;

                // Pause after correction
                let post_backspace_ms = rand::thread_rng().gen_range(100..=220);
                tokio::time::sleep(Duration::from_millis(post_backspace_ms)).await;
            }
        }

        // 2. Dispatch the correct key
        let ch_str = ch.to_string();
        press_key(page, &ch_str).await?;

        // 3. Compute base Gaussian keystroke delay
        let mut delay = sample_gaussian_delay(cfg.min_delay_ms, cfg.max_delay_ms);

        // 4. Apply cognitive & physical context modifiers
        if ch.is_uppercase() {
            // Physical shift key travel / hold delay
            delay += rand::thread_rng().gen_range(70..=150);
        } else if ch.is_ascii_digit()
            || (ch.is_ascii_punctuation() && !matches!(ch, '.' | ',' | '!' | '?' | ';' | ':'))
        {
            // Hand travel delay for top row numbers and symbols
            delay += rand::thread_rng().gen_range(50..=120);
        } else if matches!(ch, '.' | ',' | '!' | '?' | ';' | ':') {
            // Clause / sentence reading and reflection pause
            delay += rand::thread_rng().gen_range(350..=700);
        } else if ch == ' ' {
            // Natural word transition pause
            delay += rand::thread_rng().gen_range(40..=120);
        }

        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    Ok(())
}
