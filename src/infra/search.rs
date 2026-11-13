use async_trait::async_trait;
use chromiumoxide::Page;
use tracing::info;

use crate::domain::models::{InteractionRule, PeopleSearchResult, ProfileSnippet};
use crate::domain::traits::PeopleSearchService;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;

pub struct ChromiumPeopleSearchService {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumPeopleSearchService {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    pub fn resolve_network_code(name: &str) -> &'static str {
        match name.to_lowercase().as_str() {
            "first" => "F",
            "second" => "S",
            "third" => "O",
            _ => "F",
        }
    }

    pub fn resolve_geo_urn(code: &str) -> String {
        match code.to_uppercase().as_str() {
            "US" => "103644278".to_string(),
            "DE" => "101282230".to_string(),
            "PT" => "100364837".to_string(),
            "UK" => "101165590".to_string(),
            "NL" => "102890719".to_string(),
            "FR" => "105015875".to_string(),
            "LU" => "104042105".to_string(),
            "CH" => "106693272".to_string(),
            "PL" => "105072130".to_string(),
            raw if raw.chars().all(|c| c.is_ascii_digit()) => raw.to_string(),
            _ => code.to_string(),
        }
    }

    pub fn resolve_job_title_id(title: &str) -> String {
        let cleaned = title.trim().to_lowercase().replace(['-', '_'], " ");
        match cleaned.as_str() {
            "any" => "-100".to_string(),
            "owner" => "1".to_string(),
            "manager" => "2".to_string(),
            "consultant" => "3".to_string(),
            "project manager" => "4".to_string(),
            "director" => "5".to_string(),
            "president" => "6".to_string(),
            "vice president" | "founder" => "7".to_string(),
            "chief executive officer" => "8".to_string(),
            "software engineer" => "9".to_string(),
            "engineer" => "10".to_string(),
            "senior software engineer" => "39".to_string(),
            "chief technology officer" => "153".to_string(),
            "technical lead" => "189".to_string(),
            "solutions architect" => "204".to_string(),
            "platform engineer" => "6483".to_string(),
            "full stack engineer" => "25201".to_string(),
            "devops engineer" => "25764".to_string(),
            "artificial intelligence engineer" => "30128".to_string(),
            "generative ai engineer" => "31823".to_string(),
            raw if raw.lstrip_neg_digits() => raw.to_string(),
            _ => title.to_string(),
        }
    }
}

trait NegDigits {
    fn lstrip_neg_digits(&self) -> bool;
}

impl NegDigits for str {
    fn lstrip_neg_digits(&self) -> bool {
        self.strip_prefix('-')
            .unwrap_or(self)
            .chars()
            .all(|c| c.is_ascii_digit())
    }
}

#[async_trait]
impl PeopleSearchService for ChromiumPeopleSearchService {
    async fn search_people(
        &self,
        keywords: &str,
        networks: &[String],
        locations: &[String],
        _hiring_for: &[String],
        actively_hiring: bool,
        page: u32,
    ) -> Result<PeopleSearchResult, AppError> {
        info!(target: "search", "Searching people for: {keywords} (page {page}, actively_hiring={actively_hiring})");
        human_jitter(&self.rule).await;

        let search_url =
            build_people_search_url(keywords, networks, locations, actively_hiring, page);

        self.page
            .goto(&search_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: search_url.clone(),
                reason: e.to_string(),
            })?;

        crate::infra::tripwire::assert_no_tripwire(&self.page).await?;
        human_jitter(&self.rule).await;

        let js_extract = r#"
            (async () => {
                const sleep = ms => new Promise(r => setTimeout(r, ms));

                // Wait for search result cards or no-results indicator to hydrate
                let cards = [];
                for (let i = 0; i < 50; i++) {
                    cards = Array.from(document.querySelectorAll('div[role="listitem"], li.reusable-search__result-container, div.entity-result, li[class*="search-result"], div[data-chameleon-result-urn]'));
                    if (cards.length > 0) break;
                    const noResults = document.querySelector('.search-reusable-search-no-results, [data-view-name*="no-results"]');
                    if (noResults) break;
                    await sleep(200);
                }

                const seen = new Set();
                const results = [];

                for (const card of cards) {
                    const links = Array.from(card.querySelectorAll('a[href*="/in/"]'));
                    if (links.length === 0) continue;

                    // Filter out mutual connection links so candidate link is prioritized
                    let candidateLink = links.find(a => {
                        const p = a.closest('p, div');
                        const txt = (p?.innerText || '').toLowerCase();
                        return !txt.includes('mutual connection') && !txt.includes('conexão em comum');
                    }) || links[0];

                    const href = candidateLink.href;
                    if (!href.includes('/in/')) continue;

                    let path = '';
                    try {
                        path = new URL(href).pathname;
                    } catch (e) {
                        path = href;
                    }
                    const parts = path.replace(/^\/|\/$/g, '').split('/');
                    if (parts.length < 2 || parts[0] !== 'in') continue;

                    let publicId = '';
                    try {
                        publicId = decodeURIComponent(parts[1]);
                    } catch (e) {
                        publicId = parts[1];
                    }
                    if (!publicId || seen.has(publicId.toLowerCase())) continue;
                    seen.add(publicId.toLowerCase());

                    let rawName = candidateLink.innerText.trim();
                    if (rawName.includes('\n')) {
                        rawName = rawName.split('\n')[0].trim();
                    }
                    rawName = rawName.replace(/\s*•\s*(1st|2nd|3rd\+?|\d+º?)\b/gi, '').trim();
                    rawName = rawName.replace(/\s*\(.*?\)$/g, '').trim();
                    const name = rawName || publicId;

                    const semanticHeadline = card.querySelector('.entity-result__primary-subtitle, div[class*="primary-subtitle"], [data-view-name*="subtitle"]')?.innerText?.trim();
                    const semanticLocation = card.querySelector('.entity-result__secondary-subtitle, div[class*="secondary-subtitle"], [data-view-name*="caption"]')?.innerText?.trim();

                    const cardLines = (card.innerText || '')
                        .split('\n')
                        .map(l => l.trim())
                        .filter(Boolean);

                    const actionWords = new Set(['connect', 'message', 'follow', 'pending', 'conectar', 'mensagem', 'seguir', 'more', 'mais']);
                    const contentLines = cardLines.filter(line => {
                        const lower = line.toLowerCase();
                        if (actionWords.has(lower)) return false;
                        if (/^•\s*(1st|2nd|3rd\+?|\d+º?)/i.test(line)) return false;
                        if (line.includes('mutual connection') || line.includes('conexão em comum') || line.includes('conexões em comum')) return false;
                        if (line.toLowerCase().includes('view') && line.toLowerCase().includes('profile')) return false;
                        if (line.toLowerCase().includes('ver perfil')) return false;
                        if (line === rawName || line === name || line.includes(name)) return false;
                        return true;
                    });

                    const headline = semanticHeadline || (contentLines.length > 0 ? contentLines[0] : null);
                    const location = semanticLocation || (contentLines.length > 1 ? contentLines[1] : null);

                    results.push({
                        public_identifier: publicId,
                        profile_url: `https://www.linkedin.com/in/${encodeURIComponent(publicId)}/`,
                        name: name,
                        headline: headline,
                        location: location
                    });
                }
                return results;
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let profiles = eval_res
            .into_value::<Vec<ProfileSnippet>>()
            .unwrap_or_default();

        Ok(PeopleSearchResult {
            query: keywords.to_string(),
            page,
            network: networks.to_vec(),
            profiles,
        })
    }
}

pub fn build_people_search_url(
    keywords: &str,
    networks: &[String],
    locations: &[String],
    actively_hiring: bool,
    page: u32,
) -> String {
    let mut query_params = Vec::new();
    query_params.push(format!("keywords={}", urlencoding_simple(keywords)));
    query_params.push(format!("page={page}"));

    if !networks.is_empty() {
        let codes: Vec<String> = networks
            .iter()
            .map(|n| {
                format!(
                    "\"{}\"",
                    ChromiumPeopleSearchService::resolve_network_code(n)
                )
            })
            .collect();
        query_params.push(format!("network=[{}]", codes.join(",")));
    }

    if !locations.is_empty() {
        let geos: Vec<String> = locations
            .iter()
            .map(|l| format!("\"{}\"", ChromiumPeopleSearchService::resolve_geo_urn(l)))
            .collect();
        query_params.push(format!("geoUrn=[{}]", geos.join(",")));
    }

    if actively_hiring {
        query_params.push("activelyHiring=\"true\"".to_string());
    }

    format!(
        "https://www.linkedin.com/search/results/people/?{}",
        query_params.join("&")
    )
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_people_search_url_basic() {
        let url = build_people_search_url("Software Engineer", &[], &[], false, 1);
        assert_eq!(
            url,
            "https://www.linkedin.com/search/results/people/?keywords=Software+Engineer&page=1"
        );
        assert!(!url.contains("activelyHiring"));
    }

    #[test]
    fn test_build_people_search_url_with_actively_hiring() {
        let networks = vec!["second".to_string()];
        let locations = vec!["NL".to_string(), "UK".to_string()];
        let url = build_people_search_url("CTO", &networks, &locations, true, 2);

        assert!(url.contains("keywords=CTO"));
        assert!(url.contains("page=2"));
        assert!(url.contains("network=[\"S\"]"));
        assert!(url.contains("geoUrn=[\"102890719\",\"101165590\"]"));
        assert!(url.contains("activelyHiring=\"true\""));
    }

    #[test]
    fn test_build_people_search_url_without_actively_hiring() {
        let networks = vec!["second".to_string()];
        let locations = vec!["US".to_string()];
        let url = build_people_search_url("Head of AI", &networks, &locations, false, 1);

        assert!(url.contains("keywords=Head+of+AI"));
        assert!(!url.contains("activelyHiring"));
    }
}
