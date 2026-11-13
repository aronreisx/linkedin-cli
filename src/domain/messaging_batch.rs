use minijinja::Environment;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

use crate::errors::AppError;

pub const DEFAULT_MAX_BATCH_SIZE: usize = 15;
pub const DEFAULT_JITTER_MIN_SECS: u64 = 45;
pub const DEFAULT_JITTER_MAX_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderedMessage {
    pub recipient: String,
    pub text: String,
    pub utf16_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VarianceReport {
    pub total_messages: usize,
    pub unique_messages: usize,
    pub average_similarity: f64,
    pub passes_variance_check: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchDispatchPlan {
    pub total_recipients: usize,
    pub max_batch: usize,
    pub estimated_duration_secs: u64,
    pub messages: Vec<RenderedMessage>,
    pub variance: VarianceReport,
}

/// Renders a Jinja/Tera template string for each recipient context object
pub fn render_batch_templates(
    template_str: &str,
    recipients: &[Value],
) -> Result<Vec<RenderedMessage>, AppError> {
    let mut env = Environment::new();
    env.add_template("message", template_str)
        .map_err(|e| AppError::InvalidInput(format!("Invalid template syntax: {e}")))?;
    let tmpl = env
        .get_template("message")
        .map_err(|e| AppError::InvalidInput(format!("Failed to compile template: {e}")))?;

    let mut rendered = Vec::with_capacity(recipients.len());

    for (idx, recipient_val) in recipients.iter().enumerate() {
        let recipient_id = recipient_val
            .get("handle")
            .or_else(|| recipient_val.get("public_identifier"))
            .or_else(|| recipient_val.get("recipient"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let text = tmpl
            .render(recipient_val)
            .map_err(|e| {
                AppError::InvalidInput(format!(
                    "Render error for recipient #{idx} ({recipient_id}): {e}"
                ))
            })?
            .trim()
            .to_string();

        let utf16_len = text.encode_utf16().count();

        let attachment = recipient_val
            .get("attachment")
            .or_else(|| recipient_val.get("file"))
            .or_else(|| recipient_val.get("media"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        rendered.push(RenderedMessage {
            recipient: recipient_id,
            text,
            utf16_len,
            attachment,
        });
    }

    Ok(rendered)
}

/// Validates that batch size does not exceed the safe ceiling
pub fn validate_batch_limits(count: usize, max_batch: usize) -> Result<(), AppError> {
    if count == 0 {
        return Err(AppError::InvalidInput(
            "Recipient batch cannot be empty".to_string(),
        ));
    }
    if count > max_batch {
        return Err(AppError::ConnectionLimit(format!(
            "Batch size {count} exceeds safe account protection ceiling of {max_batch}. Reduce recipient count to avoid algorithmic flagging."
        )));
    }
    Ok(())
}

/// Analyzes token-level similarity across messages to detect identical spam copies
pub fn check_content_variance(messages: &[RenderedMessage]) -> Result<VarianceReport, AppError> {
    let total = messages.len();
    if total <= 1 {
        return Ok(VarianceReport {
            total_messages: total,
            unique_messages: total,
            average_similarity: 0.0,
            passes_variance_check: true,
            warning: None,
        });
    }

    let token_sets: Vec<HashSet<String>> = messages
        .iter()
        .map(|m| {
            m.text
                .to_lowercase()
                .split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .collect();

    let unique_texts: HashSet<&str> = messages.iter().map(|m| m.text.as_str()).collect();

    let mut pair_count = 0;
    let mut total_similarity = 0.0;

    for i in 0..token_sets.len() {
        for j in (i + 1)..token_sets.len() {
            let s1 = &token_sets[i];
            let s2 = &token_sets[j];

            let intersection = s1.intersection(s2).count() as f64;
            let union = s1.union(s2).count() as f64;
            let jaccard = if union == 0.0 {
                1.0
            } else {
                intersection / union
            };

            total_similarity += jaccard;
            pair_count += 1;
        }
    }

    let average_similarity = if pair_count > 0 {
        total_similarity / (pair_count as f64)
    } else {
        0.0
    };

    let passes = average_similarity < 0.95 && unique_texts.len() > 1;
    let warning = if !passes {
        Some(format!(
            "High content similarity ({:.1}%). Sending nearly identical messages risks account restriction. Consider introducing variable fields.",
            average_similarity * 100.0
        ))
    } else {
        None
    };

    Ok(VarianceReport {
        total_messages: total,
        unique_messages: unique_texts.len(),
        average_similarity,
        passes_variance_check: passes,
        warning,
    })
}

/// Generates randomized human jitter duration between dispatches
pub fn calculate_dispatch_jitter(min_secs: u64, max_secs: u64) -> u64 {
    if min_secs >= max_secs {
        return min_secs;
    }
    let mut rng = rand::thread_rng();
    rng.gen_range(min_secs..=max_secs)
}
