use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DemographicEntry {
    pub category: String,
    pub label: String,
    pub percentage: f64,
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostAnalytics {
    pub post_urn: String,
    pub impressions: u64,
    pub unique_impressions: u64,
    pub clicks: u64,
    pub reactions: u64,
    pub comments: u64,
    pub reposts: u64,
    pub engagement_rate: f64,
    pub click_through_rate: f64,
    pub top_demographics: Vec<DemographicEntry>,
    pub ingested_at: String,
}

/// Parses raw LinkedIn Voyager analytics payload into clean PostAnalytics domain struct
pub fn parse_voyager_analytics(val: &Value, post_urn: &str) -> Result<PostAnalytics, AppError> {
    let summary = val.get("summary").or_else(|| val.get("data")).unwrap_or(val);

    let impressions = summary.get("impressionCount")
        .or_else(|| summary.get("impressions"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let unique_impressions = summary.get("uniqueImpressionsCount")
        .or_else(|| summary.get("uniqueImpressions"))
        .and_then(|v| v.as_u64())
        .unwrap_or(impressions);

    let clicks = summary.get("clickCount")
        .or_else(|| summary.get("clicks"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let reactions = summary.get("reactionCount")
        .or_else(|| summary.get("likeCount"))
        .or_else(|| summary.get("reactions"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let comments = summary.get("commentCount")
        .or_else(|| summary.get("comments"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let reposts = summary.get("repostCount")
        .or_else(|| summary.get("shareCount"))
        .or_else(|| summary.get("reposts"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let total_engagements = reactions + comments + reposts + clicks;
    let engagement_rate = if impressions > 0 {
        ((total_engagements as f64) / (impressions as f64)) * 100.0
    } else {
        0.0
    };

    let click_through_rate = if impressions > 0 {
        ((clicks as f64) / (impressions as f64)) * 100.0
    } else {
        0.0
    };

    let mut demographics = Vec::new();
    if let Some(demos) = val.get("demographics").and_then(|v| v.as_array()) {
        for d in demos {
            let category = d.get("category").and_then(|v| v.as_str()).unwrap_or("General").to_string();
            let label = d.get("label").or_else(|| d.get("name")).and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
            let percentage = d.get("percentage").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let count = d.get("count").and_then(|v| v.as_u64());

            demographics.push(DemographicEntry {
                category,
                label,
                percentage,
                count,
            });
        }
    }

    Ok(PostAnalytics {
        post_urn: post_urn.to_string(),
        impressions,
        unique_impressions,
        clicks,
        reactions,
        comments,
        reposts,
        engagement_rate: (engagement_rate * 100.0).round() / 100.0,
        click_through_rate: (click_through_rate * 100.0).round() / 100.0,
        top_demographics: demographics,
        ingested_at: chrono::Utc::now().to_rfc3339(),
    })
}
