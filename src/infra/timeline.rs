use std::time::Duration;
use async_trait::async_trait;
use chromiumoxide::Page;
use serde_json::Value;
use tracing::info;

use crate::domain::timeline::{
    validate_comment_content, CommentResult, FeedSort, PostComment, ReactionResult, ReactionType,
    RepostResult, TimelinePost,
};
use crate::domain::traits::TimelineService;
use crate::domain::InteractionRule;
use crate::errors::AppError;
use crate::infra::jitter::human_jitter;
use crate::infra::scraper::ChromiumProfileScraper;
use crate::infra::tripwire::assert_no_tripwire;
use crate::infra::typing::human_type;

pub struct ChromiumTimelineService {
    page: Page,
    rule: InteractionRule,
}

impl ChromiumTimelineService {
    pub fn new(page: Page, rule: InteractionRule) -> Self {
        Self { page, rule }
    }

    /// Normalizes an activity URN string (e.g. "urn:li:activity:123456789" or raw "123456789")
    pub fn normalize_activity_urn(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("urn:li:activity:") || trimmed.starts_with("urn:li:share:") {
            trimmed.to_string()
        } else if trimmed.chars().all(|c| c.is_ascii_digit()) {
            format!("urn:li:activity:{trimmed}")
        } else if let Some(pos) = trimmed.find("activity:") {
            format!("urn:li:activity:{}", trimmed[pos + 9..].trim_end_matches('/'))
        } else {
            trimmed.to_string()
        }
    }
}

#[async_trait]
impl TimelineService for ChromiumTimelineService {
    async fn list_feed(&self, limit: usize, sort: FeedSort) -> Result<Vec<TimelinePost>, AppError> {
        let feed_url = match sort {
            FeedSort::Recent => "https://www.linkedin.com/feed/?sortBy=R",
            FeedSort::Top => "https://www.linkedin.com/feed/",
        };

        info!(target: "timeline", "Navigating to home feed ({sort}): {feed_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(feed_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: feed_url.to_string(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let mut posts = Vec::new();
        let max_scroll_attempts = (limit / 4).clamp(1, 8);

        for _ in 0..max_scroll_attempts {
            let extracted = self.extract_posts_from_current_dom().await?;
            for post in extracted {
                if !posts.iter().any(|p: &TimelinePost| p.urn == post.urn) {
                    posts.push(post);
                }
                if posts.len() >= limit {
                    break;
                }
            }

            if posts.len() >= limit {
                break;
            }

            // Smooth scroll down to trigger lazy loading of updates
            let js_scroll = r#"
                window.scrollBy({ top: 900, behavior: 'smooth' });
            "#;
            let _ = self.page.evaluate(js_scroll).await;
            tokio::time::sleep(Duration::from_millis(1200)).await;
            human_jitter(&self.rule).await;
        }

        posts.truncate(limit);
        Ok(posts)
    }

    async fn list_member_feed(&self, handle: &str, limit: usize) -> Result<Vec<TimelinePost>, AppError> {
        let public_id = ChromiumProfileScraper::extract_public_id(handle);
        let activity_url = format!("https://www.linkedin.com/in/{public_id}/recent-activity/all/");

        info!(target: "timeline", "Navigating to member activity feed for {public_id}: {activity_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&activity_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: activity_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let mut posts = Vec::new();
        let max_scroll_attempts = (limit / 4).clamp(1, 6);

        for _ in 0..max_scroll_attempts {
            let extracted = self.extract_posts_from_current_dom().await?;
            for post in extracted {
                if !posts.iter().any(|p: &TimelinePost| p.urn == post.urn) {
                    posts.push(post);
                }
                if posts.len() >= limit {
                    break;
                }
            }

            if posts.len() >= limit {
                break;
            }

            let js_scroll = r#"
                window.scrollBy({ top: 850, behavior: 'smooth' });
            "#;
            let _ = self.page.evaluate(js_scroll).await;
            tokio::time::sleep(Duration::from_millis(1000)).await;
            human_jitter(&self.rule).await;
        }

        posts.truncate(limit);
        Ok(posts)
    }

    async fn get_post(&self, raw_urn: &str) -> Result<TimelinePost, AppError> {
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Navigating to post update: {post_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let posts = self.extract_posts_from_current_dom().await?;
        if let Some(post) = posts.into_iter().find(|p| p.urn == urn) {
            Ok(post)
        } else {
            Err(AppError::NotFound(format!("Post with URN {urn} not found on page")))
        }
    }

    async fn list_comments(&self, raw_urn: &str, limit: usize) -> Result<Vec<PostComment>, AppError> {
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Fetching comments for post: {post_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let js_extract_comments = r#"
            (() => {
                const commentEls = Array.from(document.querySelectorAll(
                    '.comments-comment-item, .comments-comments-list__comment-item, article.comments-comment-item'
                ));
                return commentEls.map(el => {
                    const urn = el.getAttribute('data-id') || el.getAttribute('data-urn') || '';
                    const authorEl = el.querySelector('.comments-post-meta__name-text, .comments-post-meta__profile-link, .comments-comment-item__actor');
                    const authorName = authorEl ? authorEl.innerText.trim() : 'Unknown';
                    const linkEl = el.querySelector('a[href*="/in/"]');
                    let authorHandle = null;
                    if (linkEl && linkEl.href) {
                        const m = linkEl.href.match(/\/in\/([^/?#]+)/);
                        if (m) authorHandle = m[1];
                    }
                    const textEl = el.querySelector('.comments-comment-item__main-content, .update-components-text, .comments-comment-item-content-body');
                    const text = textEl ? textEl.innerText.trim() : '';
                    const timeEl = el.querySelector('time, .comments-comment-item__timestamp');
                    const createdAt = timeEl ? timeEl.innerText.trim() : null;

                    return {
                        urn,
                        author_name: authorName,
                        author_handle: authorHandle,
                        text,
                        created_at: createdAt
                    };
                }).filter(c => c.text.length > 0);
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract_comments)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Array(vec![]));
        let mut comments: Vec<PostComment> = serde_json::from_value(val).unwrap_or_default();
        comments.truncate(limit);
        Ok(comments)
    }

    async fn react(&self, raw_urn: &str, reaction: ReactionType) -> Result<ReactionResult, AppError> {
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Navigating to post to react ({reaction}): {post_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        // 1. Locate the reaction trigger button on the page
        let reaction_str = reaction.as_str();
        let js_trigger_reaction = format!(
            r#"
            (async () => {{
                // Find main like / reaction button
                const likeBtn = document.querySelector(
                    'button.react-button__trigger, button[aria-label*="React"], button.reactions-react-button, button[aria-label*="Reaction button state"]'
                );
                if (!likeBtn) return {{ success: false, reason: "Reaction button not found" }};

                const targetType = "{reaction_str}";
                if (targetType === "like") {{
                    likeBtn.scrollIntoView({{ behavior: "smooth", block: "center" }});
                    likeBtn.click();
                    return {{ success: true, reaction: "like" }};
                }}

                // Hover to open reaction drawer popup
                const mouseOverEvent = new MouseEvent('mouseover', {{ bubbles: true, cancelable: true, view: window }});
                likeBtn.dispatchEvent(mouseOverEvent);
                await new Promise(r => setTimeout(r, 600));

                // Find corresponding reaction option inside the drawer
                const reactionMap = {{
                    "celebrate": ['button[aria-label*="Celebrate"]', 'button[data-reaction-type="PRAISE"]'],
                    "support": ['button[aria-label*="Support"]', 'button[data-reaction-type="APPRECIATION"]'],
                    "love": ['button[aria-label*="Love"]', 'button[data-reaction-type="EMPATHY"]'],
                    "insightful": ['button[aria-label*="Insightful"]', 'button[data-reaction-type="INTEREST"]'],
                    "funny": ['button[aria-label*="Funny"]', 'button[data-reaction-type="ENTERTAINMENT"]']
                }};

                const selectors = reactionMap[targetType] || [];
                let foundBtn = null;
                for (const sel of selectors) {{
                    const el = document.querySelector(sel);
                    if (el) {{ foundBtn = el; break; }}
                }}

                if (foundBtn) {{
                    foundBtn.click();
                    return {{ success: true, reaction: targetType }};
                }} else {{
                    // Fallback to simple click on the like button
                    likeBtn.click();
                    return {{ success: true, reaction: "like", fallback: true }};
                }}
            }})()
            "#
        );

        let eval_res = self
            .page
            .evaluate(js_trigger_reaction)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let res_val: Value = eval_res.into_value().unwrap_or_default();
        let success = res_val["success"].as_bool().unwrap_or(false);

        if !success {
            let reason = res_val["reason"].as_str().unwrap_or("Failed to click reaction element");
            return Err(AppError::Browser(format!("React failed for {urn}: {reason}")));
        }

        human_jitter(&self.rule).await;

        Ok(ReactionResult {
            post_urn: urn,
            action: "react".to_string(),
            reaction: Some(reaction),
            success: true,
        })
    }

    async fn unreact(&self, raw_urn: &str) -> Result<ReactionResult, AppError> {
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Removing reaction from post: {post_url}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        let js_unreact = r#"
            (() => {
                const likeBtn = document.querySelector(
                    'button.react-button__trigger, button[aria-label*="React"], button.reactions-react-button'
                );
                if (!likeBtn) return { success: false, reason: "Reaction button not found" };

                const isPressed = likeBtn.getAttribute('aria-pressed') === 'true'
                    || likeBtn.classList.contains('react-button--active')
                    || (likeBtn.getAttribute('aria-label') || '').toLowerCase().includes('unreact');

                if (isPressed) {
                    likeBtn.click();
                    return { success: true, unreacted: true };
                }
                return { success: true, unreacted: false, note: "Post was not reacted to" };
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_unreact)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let res_val: Value = eval_res.into_value().unwrap_or_default();
        let success = res_val["success"].as_bool().unwrap_or(false);

        if !success {
            let reason = res_val["reason"].as_str().unwrap_or("Failed to unreact to post");
            return Err(AppError::Browser(format!("Unreact failed for {urn}: {reason}")));
        }

        human_jitter(&self.rule).await;

        Ok(ReactionResult {
            post_urn: urn,
            action: "unreact".to_string(),
            reaction: None,
            success: true,
        })
    }

    async fn add_comment(&self, raw_urn: &str, text: &str) -> Result<CommentResult, AppError> {
        validate_comment_content(text)?;
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Adding comment to post {urn}");
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        // 1. Focus comment box
        let comment_box_selector = ".editor-content p, div[role='textbox'], .comments-comment-box__editor";
        let js_focus = format!(
            r#"
            (() => {{
                const el = document.querySelector("{comment_box_selector}");
                if (!el) return false;
                el.scrollIntoView({{ behavior: "smooth", block: "center" }});
                el.focus();
                return true;
            }})()
            "#
        );

        let can_focus: bool = self
            .page
            .evaluate(js_focus)
            .await
            .map(|r| r.into_value().unwrap_or(false))
            .unwrap_or(false);

        if !can_focus {
            return Err(AppError::Browser(format!(
                "Comment input box not found for post {urn}"
            )));
        }

        tokio::time::sleep(Duration::from_millis(400)).await;

        // 2. Type comment text with human cadence
        let typing_cfg = crate::infra::typing::HumanTypingConfig::default();
        human_type(&self.page, text, Some(&typing_cfg)).await?;

        human_jitter(&self.rule).await;

        // 3. Click submit button
        let js_submit = r#"
            (() => {
                const submitBtn = document.querySelector(
                    'button.comments-comment-box__submit-button, button.comments-comment-box__submit-button--active, button[type="submit"]'
                );
                if (submitBtn && !submitBtn.disabled) {
                    submitBtn.click();
                    return true;
                }
                return false;
            })()
        "#;

        let submitted: bool = self
            .page
            .evaluate(js_submit)
            .await
            .map(|r| r.into_value().unwrap_or(false))
            .unwrap_or(false);

        if !submitted {
            return Err(AppError::Browser(format!(
                "Failed to submit comment button for post {urn}"
            )));
        }

        human_jitter(&self.rule).await;

        Ok(CommentResult {
            post_urn: urn,
            comment_urn: None,
            text: text.to_string(),
            success: true,
        })
    }

    async fn repost(&self, raw_urn: &str, commentary: Option<&str>) -> Result<RepostResult, AppError> {
        let urn = Self::normalize_activity_urn(raw_urn);
        let post_url = format!("https://www.linkedin.com/feed/update/{urn}/");

        info!(target: "timeline", "Reposting post {urn} (with commentary: {})", commentary.is_some());
        assert_no_tripwire(&self.page).await?;

        self.page
            .goto(&post_url)
            .await
            .map_err(|e| AppError::NavigationFailed {
                url: post_url.clone(),
                reason: e.to_string(),
            })?;

        human_jitter(&self.rule).await;
        assert_no_tripwire(&self.page).await?;

        // 1. Click repost action button to trigger popover
        let js_open_repost = r#"
            (() => {
                const repostBtn = Array.from(document.querySelectorAll('button')).find(b => {
                    const label = (b.getAttribute('aria-label') || b.innerText || '').toLowerCase();
                    return label.includes('repost') || label.includes('share');
                });
                if (!repostBtn) return false;
                repostBtn.click();
                return true;
            })()
        "#;

        let opened: bool = self
            .page
            .evaluate(js_open_repost)
            .await
            .map(|r| r.into_value().unwrap_or(false))
            .unwrap_or(false);

        if !opened {
            return Err(AppError::Browser(format!(
                "Repost trigger button not found for post {urn}"
            )));
        }

        tokio::time::sleep(Duration::from_millis(600)).await;

        if let Some(comment_text) = commentary {
            // Quote repost with thoughts
            let js_click_with_thoughts = r#"
                (() => {
                    const btn = Array.from(document.querySelectorAll('div, button, li')).find(el => {
                        const t = (el.innerText || '').toLowerCase();
                        return t.includes('repost with your thoughts') || t.includes('share with thoughts');
                    });
                    if (btn) { btn.click(); return true; }
                    return false;
                })()
            "#;

            let _ = self.page.evaluate(js_click_with_thoughts).await;
            tokio::time::sleep(Duration::from_millis(800)).await;

            let typing_cfg = crate::infra::typing::HumanTypingConfig::default();
            human_type(&self.page, comment_text, Some(&typing_cfg)).await?;

            human_jitter(&self.rule).await;

            let js_submit_quote = r#"
                (() => {
                    const postBtn = Array.from(document.querySelectorAll('button')).find(b => {
                        const t = (b.innerText || '').trim().toLowerCase();
                        return t === 'post' || t === 'share';
                    });
                    if (postBtn && !postBtn.disabled) {
                        postBtn.click();
                        return true;
                    }
                    return false;
                })()
            "#;

            let _ = self.page.evaluate(js_submit_quote).await;
        } else {
            // Instant repost without thoughts
            let js_instant_repost = r#"
                (() => {
                    const btn = Array.from(document.querySelectorAll('div, button, li')).find(el => {
                        const t = (el.innerText || '').toLowerCase();
                        return t.includes('repost instantly') || t.includes('repost without thoughts');
                    });
                    if (btn) { btn.click(); return true; }
                    return false;
                })()
            "#;

            let _ = self.page.evaluate(js_instant_repost).await;
        }

        human_jitter(&self.rule).await;

        Ok(RepostResult {
            post_urn: urn,
            commentary: commentary.map(|s| s.to_string()),
            repost_urn: None,
            success: true,
        })
    }
}

impl ChromiumTimelineService {
    async fn extract_posts_from_current_dom(&self) -> Result<Vec<TimelinePost>, AppError> {
        let js_extract = r#"
            (() => {
                let postNodes = Array.from(document.querySelectorAll(
                    'div[data-urn*="urn:li:activity"], div[data-id*="urn:li:activity"], .feed-shared-update-v2'
                ));

                if (postNodes.length === 0) {
                    postNodes = Array.from(document.querySelectorAll('div[role="listitem"]')).filter(
                        c => c.querySelector('button[aria-label*="Reaction button state"]') || c.querySelector('#thumbs-up-outline-feed-compact')
                    );
                }

                return postNodes.map(node => {
                    const rawUrn = node.getAttribute('data-urn') || node.getAttribute('data-id') || '';
                    const m = rawUrn.match(/urn:li:(activity|share):[0-9]+/);
                    let urn = m ? m[0] : (rawUrn || '');
                    if (!urn) {
                        const key = node.getAttribute('componentkey') || node.id || '';
                        if (key) {
                            urn = key.startsWith('urn:li:') ? key : `urn:li:activity:${key}`;
                        } else {
                            urn = 'urn:li:activity:0';
                        }
                    }

                    // Author info
                    const authorLinkEl = node.querySelector('a[href*="/in/"]');
                    let authorHandle = null;
                    if (authorLinkEl && authorLinkEl.href) {
                        const handleMatch = authorLinkEl.href.match(/\/in\/([^/?#]+)/);
                        if (handleMatch) authorHandle = handleMatch[1];
                    }

                    const authorNameEl = node.querySelector(
                        '.update-components-actor__name, .feed-shared-actor__name, span.update-components-actor__single-line-truncate, h2, h3, strong'
                    );
                    let authorName = authorNameEl ? authorNameEl.innerText.trim() : 'Unknown';
                    if ((authorName === 'Feed post' || authorName === 'Unknown') && authorLinkEl) {
                        const firstLine = authorLinkEl.innerText.trim().split('\n')[0].trim();
                        if (firstLine) authorName = firstLine;
                        else if (authorHandle) authorName = authorHandle;
                    }

                    const headlineEl = node.querySelector(
                        '.update-components-actor__description, .feed-shared-actor__description, .update-components-actor__sub-description'
                    );
                    let authorHeadline = headlineEl ? headlineEl.innerText.trim() : null;
                    if (!authorHeadline) {
                        // Extract headline snippet from card text lines
                        const lines = node.innerText.split('\n').map(l => l.trim()).filter(l => l.length > 0);
                        const nameIdx = lines.indexOf(authorName);
                        if (nameIdx >= 0 && nameIdx + 1 < lines.length) {
                            const nextLine = lines[nameIdx + 1];
                            if (!nextLine.startsWith('•') && !nextLine.includes('Follow') && nextLine.length > 5) {
                                authorHeadline = nextLine;
                            } else if (nameIdx + 2 < lines.length) {
                                const lineAfter = lines[nameIdx + 2];
                                if (!lineAfter.includes('Follow') && lineAfter.length > 5) {
                                    authorHeadline = lineAfter;
                                }
                            }
                        }
                    }

                    // Post text content
                    const textEl = node.querySelector(
                        '.feed-shared-update-v2__description, .update-components-text, span.break-words, div[dir="ltr"]'
                    );
                    const text = textEl ? textEl.innerText.trim() : node.innerText.slice(0, 500).trim();

                    // Time
                    const timeEl = node.querySelector('time, .update-components-actor__sub-description');
                    const createdAt = timeEl ? timeEl.innerText.trim() : null;

                    // Social counts
                    const reactionsEl = node.querySelector(
                        '.social-details-social-counts__reactions-count, button[aria-label*="reaction"], button[aria-label*="Reaction button state"]'
                    );
                    let reactionCount = 0;
                    if (reactionsEl) {
                        const countStr = reactionsEl.innerText.replace(/[^0-9]/g, '');
                        if (countStr) reactionCount = parseInt(countStr, 10) || 0;
                    }

                    const commentsEl = node.querySelector(
                        '.social-details-social-counts__comments, button[aria-label*="comment"], button[aria-label*="Comment"]'
                    );
                    let commentCount = 0;
                    if (commentsEl) {
                        const countStr = commentsEl.innerText.replace(/[^0-9]/g, '');
                        if (countStr) commentCount = parseInt(countStr, 10) || 0;
                    }

                    const repostsEl = node.querySelector(
                        '.social-details-social-counts__reposts, button[aria-label*="repost"], button[aria-label*="Repost"]'
                    );
                    let repostCount = 0;
                    if (repostsEl) {
                        const countStr = repostsEl.innerText.replace(/[^0-9]/g, '');
                        if (countStr) repostCount = parseInt(countStr, 10) || 0;
                    }

                    // My reaction status
                    let myReaction = null;
                    const likeBtn = node.querySelector(
                        'button.react-button__trigger, button.reactions-react-button, button[aria-label*="React"], button[aria-label*="Reaction button state"]'
                    );
                    if (likeBtn) {
                        const ariaLabel = (likeBtn.getAttribute('aria-label') || '').toLowerCase();
                        if (ariaLabel.includes('celebrate')) myReaction = 'celebrate';
                        else if (ariaLabel.includes('support')) myReaction = 'support';
                        else if (ariaLabel.includes('love')) myReaction = 'love';
                        else if (ariaLabel.includes('insightful')) myReaction = 'insightful';
                        else if (ariaLabel.includes('funny')) myReaction = 'funny';
                        else if (likeBtn.getAttribute('aria-pressed') === 'true' || ariaLabel.includes('state: like') || ariaLabel.includes('liked')) myReaction = 'like';
                    }

                    // Media URLs
                    const mediaEls = Array.from(node.querySelectorAll('img.feed-shared-image__image, img[src*="media.licdn.com"]'));
                    const mediaUrls = mediaEls.map(img => img.src).filter(src => !!src && !src.includes('profile-displayphoto'));

                    return {
                        urn,
                        author_name: authorName,
                        author_handle: authorHandle,
                        author_headline: authorHeadline,
                        text,
                        created_at: createdAt,
                        reaction_count: reactionCount,
                        comment_count: commentCount,
                        repost_count: repostCount,
                        my_reaction: myReaction,
                        media_urls: mediaUrls
                    };
                }).filter(p => p.urn && p.urn.length > 0);
            })()
        "#;

        let eval_res = self
            .page
            .evaluate(js_extract)
            .await
            .map_err(|e| AppError::Cdp(e.to_string()))?;

        let val: Value = eval_res.into_value().unwrap_or(Value::Array(vec![]));
        let posts: Vec<TimelinePost> = serde_json::from_value(val).unwrap_or_default();
        Ok(posts)
    }
}
