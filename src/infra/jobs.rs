use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use std::time::Duration;
use tracing::info;

use crate::domain::jobs::{
    JobDetails, JobSaveResult, JobSearchFilter, JobSearchResult, JobSummary,
};
use crate::domain::models::InteractionRule;
use crate::domain::traits::JobService;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;
use crate::infra::tripwire::assert_no_tripwire;

pub struct ChromiumJobService {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumJobService {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    /// Normalizes a job identifier or full LinkedIn job posting URL to numeric job ID
    pub fn normalize_job_id(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("urn:li:jobPosting:") {
            return trimmed.trim_start_matches("urn:li:jobPosting:").to_string();
        }

        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            return trimmed.to_string();
        }

        // Match /jobs/view/<id> or /view/<id>
        if let Some(pos) = trimmed.find("/view/") {
            let sub = &trimmed[pos + 6..];
            let id: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() {
                return id;
            }
        }

        // Match currentJobId=<id>
        if let Some(pos) = trimmed.find("currentJobId=") {
            let sub = &trimmed[pos + 13..];
            let id: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() {
                return id;
            }
        }

        trimmed.to_string()
    }

    /// Extract job summary list from current search or saved jobs page DOM
    async fn extract_jobs_from_dom(&self) -> Result<Vec<JobSummary>, AppError> {
        let js_extract = r#"
            (() => {
                const jobNodes = Array.from(document.querySelectorAll(
                    'div.job-card-container, li.jobs-search-results__list-item, div[data-job-id], .jobs-search-results-list__list-item, div.jobs-unified-top-card, div[role="listitem"]'
                )).filter(node => {
                    const hasJobLink = !!node.querySelector('a[href*="/jobs/view/"], a.job-card-list__title, a.job-card-container__link');
                    const hasId = !!(node.getAttribute('data-job-id') || (node.getAttribute('data-occludable-job-id')));
                    return hasJobLink || hasId;
                });

                return jobNodes.map(node => {
                    // Extract job ID and URL
                    let id = node.getAttribute('data-job-id') || node.getAttribute('data-occludable-job-id') || '';
                    const linkEl = node.querySelector('a[href*="/jobs/view/"], a.job-card-list__title, a.job-card-container__link');
                    let jobUrl = '';
                    if (linkEl && linkEl.href) {
                        jobUrl = linkEl.href.split('?')[0];
                        if (!id) {
                            const m = jobUrl.match(/\/view\/([0-9]+)/);
                            if (m) id = m[1];
                        }
                    }

                    if (!id) {
                        const m = node.innerHTML.match(/urn:li:jobPosting:([0-9]+)/);
                        if (m) id = m[1];
                    }

                    if (!id) id = '0';
                    if (!jobUrl) jobUrl = `https://www.linkedin.com/jobs/view/${id}/`;

                    // Title
                    const titleEl = node.querySelector(
                        'a.job-card-list__title--link, a.job-card-list__title, .artdeco-entity-lockup__title, h3, strong'
                    );
                    let title = titleEl ? titleEl.innerText.trim() : 'Unknown Position';
                    title = title.split('\n')[0].trim();

                    // Company name & handle
                    const companyEl = node.querySelector(
                        '.job-card-container__primary-description, .artdeco-entity-lockup__subtitle, a[href*="/company/"]'
                    );
                    let companyName = companyEl ? companyEl.innerText.trim() : 'Unknown Company';
                    companyName = companyName.split('\n')[0].trim();

                    let companyHandle = null;
                    const companyLink = node.querySelector('a[href*="/company/"]');
                    if (companyLink && companyLink.href) {
                        const m = companyLink.href.match(/\/company\/([^/?#]+)/);
                        if (m) companyHandle = m[1];
                    }

                    // Location & Workplace type
                    const locEl = node.querySelector(
                        '.job-card-container__metadata-item, .artdeco-entity-lockup__caption, ul.job-card-container__metadata-wrapper li'
                    );
                    const location = locEl ? locEl.innerText.trim() : 'Unspecified';

                    let workplaceType = null;
                    const lowerText = node.innerText.toLowerCase();
                    if (lowerText.includes('remote')) workplaceType = 'Remote';
                    else if (lowerText.includes('hybrid')) workplaceType = 'Hybrid';
                    else if (lowerText.includes('on-site') || lowerText.includes('onsite')) workplaceType = 'On-site';

                    // Easy apply
                    const isEasyApply = lowerText.includes('easy apply') || !!node.querySelector('.job-card-container__apply-method');

                    // Time posted
                    const timeEl = node.querySelector('time, .job-card-container__footer-item');
                    const postedAt = timeEl ? timeEl.innerText.trim() : null;

                    return {
                        id,
                        urn: `urn:li:jobPosting:${id}`,
                        title,
                        company_name: companyName,
                        company_handle: companyHandle,
                        location,
                        workplace_type: workplaceType,
                        posted_at: postedAt,
                        is_easy_apply: isEasyApply,
                        job_url: jobUrl
                    };
                }).filter(j => j.id !== '0' && j.title.length > 0);
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Array(vec![]));
        let jobs: Vec<JobSummary> = serde_json::from_value(val).unwrap_or_default();
        Ok(jobs)
    }
}

#[async_trait]
impl JobService for ChromiumJobService {
    async fn search_jobs(&self, filter: &JobSearchFilter) -> Result<JobSearchResult, AppError> {
        let search_url = filter.build_url();
        info!(target: "jobs", "Navigating to LinkedIn jobs search: {search_url}");

        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&search_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: search_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let mut jobs = Vec::new();
        let max_scrolls = (filter.limit / 5).clamp(1, 6);

        for _ in 0..max_scrolls {
            let extracted = self.extract_jobs_from_dom().await?;
            for job in extracted {
                if !jobs.iter().any(|j: &JobSummary| j.id == job.id) {
                    jobs.push(job);
                }
                if jobs.len() >= filter.limit {
                    break;
                }
            }

            if jobs.len() >= filter.limit {
                break;
            }

            // Scroll jobs result rail
            let js_scroll = r#"
                (() => {
                    const rail = document.querySelector('.jobs-search-results-list, .scaffold-layout__list') || window;
                    rail.scrollBy({ top: 800, behavior: 'smooth' });
                })()
            "#;
            let _ = self.page.evaluate(js_scroll).await;
            tokio::time::sleep(Duration::from_millis(1000)).await;
            human_jitter(&self.rule).await;
        }

        jobs.truncate(filter.limit);
        let total = jobs.len();

        Ok(JobSearchResult {
            jobs,
            total,
            page: filter.page,
            query: filter.keywords.clone(),
        })
    }

    async fn get_job(&self, job_id_or_url: &str) -> Result<JobDetails, AppError> {
        let job_id = Self::normalize_job_id(job_id_or_url);
        let job_url = format!("https://www.linkedin.com/jobs/view/{job_id}/");

        info!(target: "jobs", "Inspecting LinkedIn job posting: {job_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&job_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: job_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        // Click "See more" or expand description button if truncated
        let js_expand = r#"
            (() => {
                const btn = document.querySelector(
                    'button[aria-label*="See more description"], button.jobs-description__footer-button, button.show-more-less-html__button--more'
                );
                if (btn) btn.click();
            })()
        "#;
        let _ = self.page.evaluate(js_expand).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        let js_extract_details = format!(
            r#"
            (() => {{
                const jobId = "{job_id}";
                const jobUrl = "{job_url}";

                // Title
                const titleEl = document.querySelector(
                    'h1.top-card-layout__title, h1.t-24, h1.job-details-jobs-unified-top-card__job-title, h1'
                );
                const title = titleEl ? titleEl.innerText.trim() : 'Unknown Position';

                // Company
                const companyLink = document.querySelector(
                    'a.topcard__org-name-link, a[href*="/company/"], .job-details-jobs-unified-top-card__company-name a'
                );
                let companyName = companyLink ? companyLink.innerText.trim() : '';
                let companyUrl = companyLink ? companyLink.href.split('?')[0] : null;
                let companyHandle = null;
                if (companyUrl) {{
                    const m = companyUrl.match(/\/company\/([^/?#]+)/);
                    if (m) companyHandle = m[1];
                }}
                if (!companyName) {{
                    const companyFallback = document.querySelector(
                        '.job-details-jobs-unified-top-card__company-name, .topcard__flavor'
                    );
                    if (companyFallback) companyName = companyFallback.innerText.trim();
                }}

                // Location & workplace
                const locEl = document.querySelector(
                    '.topcard__flavor--bullet, .job-details-jobs-unified-top-card__primary-description-container span'
                );
                const location = locEl ? locEl.innerText.trim() : 'Unspecified';

                let workplaceType = null;
                const pageText = document.body.innerText.toLowerCase();
                if (pageText.includes('remote')) workplaceType = 'Remote';
                else if (pageText.includes('hybrid')) workplaceType = 'Hybrid';
                else if (pageText.includes('on-site') || pageText.includes('onsite')) workplaceType = 'On-site';

                // Time posted & applicant count
                let postedAt = null;
                const timeEl = document.querySelector('time, .posted-time-ago__text');
                if (timeEl) postedAt = timeEl.innerText.trim();

                let applicantCount = null;
                const applicantEl = document.querySelector(
                    '.num-applicants__caption, .jobs-unified-top-card__applicant-count'
                );
                if (applicantEl) {{
                    const m = applicantEl.innerText.match(/([0-9]+)/);
                    if (m) applicantCount = parseInt(m[1], 10);
                }}

                // Easy apply check
                const isEasyApply = !!document.querySelector(
                    'button.jobs-apply-button[aria-label*="Easy Apply"], button.jobs-apply-button--top-card'
                ) || pageText.includes('easy apply');

                // Salary range
                let salaryRange = null;
                const salaryEl = document.querySelector(
                    '.job-details-preferences-and-skills__pill, .compensation__salary, .jobs-unified-top-card__job-insight'
                );
                if (salaryEl && salaryEl.innerText.includes('$')) {{
                    salaryRange = salaryEl.innerText.trim();
                }}

                // Employment type & experience level
                let employmentType = null;
                let experienceLevel = null;
                const insights = Array.from(document.querySelectorAll('.job-details-jobs-unified-top-card__job-insight, .description__job-criteria-item'));
                for (const ins of insights) {{
                    const t = ins.innerText.toLowerCase();
                    if (t.includes('full-time')) employmentType = 'Full-time';
                    else if (t.includes('part-time')) employmentType = 'Part-time';
                    else if (t.includes('contract')) employmentType = 'Contract';

                    if (t.includes('entry level')) experienceLevel = 'Entry level';
                    else if (t.includes('mid-senior')) experienceLevel = 'Mid-Senior level';
                    else if (t.includes('associate')) experienceLevel = 'Associate';
                    else if (t.includes('director')) experienceLevel = 'Director';
                }}

                // Hiring team members
                const hiringTeam = [];
                const recruiterCards = Array.from(document.querySelectorAll('.hirer-card__hirer-information, .jobs-poster__name'));
                for (const rc of recruiterCards) {{
                    const link = rc.querySelector('a[href*="/in/"]');
                    const name = link ? link.innerText.trim() : rc.innerText.trim().split('\n')[0];
                    let handle = null;
                    let profileUrl = null;
                    if (link && link.href) {{
                        profileUrl = link.href.split('?')[0];
                        const m = profileUrl.match(/\/in\/([^/?#]+)/);
                        if (m) handle = m[1];
                    }}
                    const headlineEl = rc.querySelector('.hirer-card__hirer-job-title');
                    const headline = headlineEl ? headlineEl.innerText.trim() : null;

                    if (name) {{
                        hiringTeam.push({{ name, handle, headline, profile_url: profileUrl }});
                    }}
                }}

                // Skills
                const skills = [];
                const skillEls = Array.from(document.querySelectorAll('.job-details-preferences-and-skills__pill, .job-details-skill-match-status-list__item'));
                for (const s of skillEls) {{
                    const txt = s.innerText.trim();
                    if (txt && !skills.includes(txt)) skills.push(txt);
                }}

                // Description
                const descEl = document.querySelector(
                    '#job-details, .jobs-description__content, .jobs-box__htmlContent, .show-more-less-html__markup'
                );
                const descriptionMarkdown = descEl ? descEl.innerText.trim() : '';

                return {{
                    id: jobId,
                    urn: `urn:li:jobPosting:${{jobId}}`,
                    title,
                    company: {{
                        name: companyName || 'Unknown Company',
                        handle: companyHandle,
                        url: companyUrl
                    }},
                    location,
                    workplace_type: workplaceType,
                    posted_at: postedAt,
                    applicant_count: applicantCount,
                    is_easy_apply: isEasyApply,
                    salary_range: salaryRange,
                    employment_type: employmentType,
                    experience_level: experienceLevel,
                    required_skills: skills,
                    hiring_team: hiringTeam,
                    description_markdown: descriptionMarkdown,
                    job_url: jobUrl
                }};
            }})()
            "#
        );

        let eval_res = self
            .page
            .evaluate(js_extract_details)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Null);
        if val.is_null() {
            return Err(AppError::Browser(format!(
                "Failed to parse job details for ID {job_id}"
            )));
        }

        let details: JobDetails = serde_json::from_value(val)
            .map_err(AppError::Json)?;

        Ok(details)
    }

    async fn save_job(&self, job_id_or_url: &str) -> Result<JobSaveResult, AppError> {
        let job_id = Self::normalize_job_id(job_id_or_url);
        let job_url = format!("https://www.linkedin.com/jobs/view/{job_id}/");

        info!(target: "jobs", "Saving LinkedIn job posting: {job_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&job_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: job_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let js_save = r#"
            (() => {
                const saveBtn = document.querySelector(
                    'button.jobs-save-button, button[aria-label*="Save job"], button[aria-label*="Save"], button.artdeco-button[data-control-name="save_job"]'
                );
                if (!saveBtn) return { success: false, reason: "Save button not found" };

                const label = (saveBtn.getAttribute('aria-label') || saveBtn.innerText || '').toLowerCase();
                if (label.includes('saved') || label.includes('unsave')) {
                    return { success: true, already_saved: true };
                }

                saveBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                saveBtn.click();
                return { success: true, already_saved: false };
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_save)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Null);
        let success = val.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        let already_saved = val.get("already_saved").and_then(|v| v.as_bool()).unwrap_or(false);

        if !success {
            let reason = val.get("reason").and_then(|v| v.as_str()).unwrap_or("Action failed");
            return Err(AppError::Browser(format!("Failed to save job {job_id}: {reason}")));
        }

        human_jitter(&self.rule).await;

        Ok(JobSaveResult {
            job_id,
            saved: true,
            success: true,
            message: if already_saved {
                "Job was already in saved items".to_string()
            } else {
                "Job successfully saved".to_string()
            },
        })
    }

    async fn unsave_job(&self, job_id_or_url: &str) -> Result<JobSaveResult, AppError> {
        let job_id = Self::normalize_job_id(job_id_or_url);
        let job_url = format!("https://www.linkedin.com/jobs/view/{job_id}/");

        info!(target: "jobs", "Unsaving LinkedIn job posting: {job_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&job_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: job_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let js_unsave = r#"
            (() => {
                const saveBtn = document.querySelector(
                    'button.jobs-save-button, button[aria-label*="Unsave job"], button[aria-label*="Saved"], button[aria-label*="Save"]'
                );
                if (!saveBtn) return { success: false, reason: "Button not found" };

                const label = (saveBtn.getAttribute('aria-label') || saveBtn.innerText || '').toLowerCase();
                if (!label.includes('saved') && !label.includes('unsave')) {
                    return { success: true, was_not_saved: true };
                }

                saveBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                saveBtn.click();
                return { success: true, was_not_saved: false };
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_unsave)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Null);
        let success = val.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        let was_not_saved = val.get("was_not_saved").and_then(|v| v.as_bool()).unwrap_or(false);

        if !success {
            let reason = val.get("reason").and_then(|v| v.as_str()).unwrap_or("Action failed");
            return Err(AppError::Browser(format!("Failed to unsave job {job_id}: {reason}")));
        }

        human_jitter(&self.rule).await;

        Ok(JobSaveResult {
            job_id,
            saved: false,
            success: true,
            message: if was_not_saved {
                "Job was not in saved items".to_string()
            } else {
                "Job successfully removed from saved items".to_string()
            },
        })
    }

    async fn list_saved_jobs(&self, limit: usize) -> Result<Vec<JobSummary>, AppError> {
        let saved_url = "https://www.linkedin.com/my-items/saved-jobs/";
        info!(target: "jobs", "Listing saved jobs from: {saved_url}");

        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(saved_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: saved_url.to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let mut jobs = Vec::new();
        let max_scrolls = (limit / 4).clamp(1, 5);

        for _ in 0..max_scrolls {
            let extracted = self.extract_jobs_from_dom().await?;
            for job in extracted {
                if !jobs.iter().any(|j: &JobSummary| j.id == job.id) {
                    jobs.push(job);
                }
                if jobs.len() >= limit {
                    break;
                }
            }

            if jobs.len() >= limit {
                break;
            }

            let js_scroll = r#"
                window.scrollBy({ top: 800, behavior: 'smooth' });
            "#;
            let _ = self.page.evaluate(js_scroll).await;
            tokio::time::sleep(Duration::from_millis(1000)).await;
            human_jitter(&self.rule).await;
        }

        jobs.truncate(limit);
        Ok(jobs)
    }
}
