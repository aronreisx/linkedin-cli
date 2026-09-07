use std::sync::Arc;
use chromiumoxide::browser::{Browser, BrowserConfig as CdpBrowserConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use crate::browser::config::BrowserConfig;
use crate::errors::AppError;

pub struct BrowserSession {
    browser: Arc<Mutex<Option<Browser>>>,
    is_remote: bool,
    session_name: String,
    slow_mo_ms: Option<u64>,
}

impl BrowserSession {
    pub async fn launch(config: &BrowserConfig) -> Result<Self, AppError> {
        // 1. If an explicit CDP websocket or port URL is provided, attach to it.
        if let Some(ref cdp_url) = config.cdp_url {
            info!(target: "browser", "Connecting to existing CDP endpoint at {cdp_url}...");
            let (browser, mut handler) = Browser::connect(cdp_url)
                .await
                .map_err(|e| AppError::Browser(format!("Failed to connect to CDP endpoint {cdp_url}: {e}")))?;

            tokio::spawn(async move {
                while let Some(event) = handler.next().await {
                    if let Err(e) = event {
                        debug!(target: "browser", "CDP handler event error: {e}");
                    }
                }
            });

            return Ok(Self {
                browser: Arc::new(Mutex::new(Some(browser))),
                is_remote: true,
                session_name: config.session_name.clone(),
                slow_mo_ms: config.slow_mo_ms,
            });
        }

        // 2. Check for an active running session daemon in the registry
        if let Some(active) = crate::browser::registry::find_active_session(&config.session_name) {
            info!(target: "browser", "Attached to running session '{}' at {}", active.name, active.endpoint);
            match Browser::connect(&active.endpoint).await {
                Ok((browser, mut handler)) => {
                    tokio::spawn(async move {
                        while let Some(event) = handler.next().await {
                            if let Err(e) = event {
                                debug!(target: "browser", "CDP handler event error: {e}");
                            }
                        }
                    });

                    return Ok(Self {
                        browser: Arc::new(Mutex::new(Some(browser))),
                        is_remote: true,
                        session_name: config.session_name.clone(),
                        slow_mo_ms: config.slow_mo_ms,
                    });
                }
                Err(e) => {
                    warn!(target: "browser", "Failed to connect to active session '{}' ({e}); pruning stale entry and launching new instance...", active.name);
                    let _ = crate::browser::registry::clear_session(&active.name);
                }
            }
        }

        // 3. Otherwise launch a managed Chromium instance
        let user_data_dir = config.resolve_user_data_dir();
        std::fs::create_dir_all(&user_data_dir).map_err(AppError::Io)?;

        let mut builder = CdpBrowserConfig::builder()
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-features=IsolateOrigins,site-per-process")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-infobars")
            .arg("--window-size=1280,800")
            .user_data_dir(&user_data_dir);

        if let Some(executable) = resolve_chromium_executable() {
            info!(target: "browser", "Using Chromium executable at {}", executable.display());
            builder = builder.chrome_executable(executable);
        }

        if config.headed {
            builder = builder.with_head();
        }

        let cdp_config = builder
            .build()
            .map_err(|e| AppError::Browser(format!("Failed to build browser config: {e}")))?;

        info!(target: "browser", "Launching Chromium instance with profile at {}...", user_data_dir.display());
        let (browser, mut handler) = Browser::launch(cdp_config)
            .await
            .map_err(|e| AppError::Browser(format!("Failed to launch browser: {e}")))?;

        let browser_arc = Arc::new(Mutex::new(Some(browser)));

        // Background handler for CDP messages
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    debug!(target: "browser", "CDP handler event error: {e}");
                }
            }
        });

        // Background task for clean termination on SIGINT / SIGTERM
        let sig_browser = Arc::clone(&browser_arc);
        tokio::spawn(async move {
            #[cfg(unix)]
            {
                let mut sigterm = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        warn!(target: "browser", "Failed to register SIGTERM listener: {e}");
                        None
                    }
                };

                tokio::select! {
                    _ = signal::ctrl_c() => {
                        debug!(target: "browser", "Received SIGINT, closing browser cleanly...");
                        let mut lock = sig_browser.lock().await;
                        if let Some(mut b) = lock.take() {
                            let _ = b.close().await;
                            let _ = b.wait().await;
                        }
                        std::process::exit(130);
                    }
                    _ = async {
                        if let Some(ref mut s) = sigterm {
                            s.recv().await;
                        } else {
                            futures::future::pending::<()>().await;
                        }
                    } => {
                        debug!(target: "browser", "Received SIGTERM, closing browser cleanly...");
                        let mut lock = sig_browser.lock().await;
                        if let Some(mut b) = lock.take() {
                            let _ = b.close().await;
                            let _ = b.wait().await;
                        }
                        std::process::exit(143);
                    }
                }
            }

            #[cfg(not(unix))]
            {
                let _ = signal::ctrl_c().await;
                debug!(target: "browser", "Received Ctrl-C, closing browser cleanly...");
                let mut lock = sig_browser.lock().await;
                if let Some(mut b) = lock.take() {
                    let _ = b.close().await;
                    let _ = b.wait().await;
                }
                std::process::exit(130);
            }
        });

        Ok(Self {
            browser: browser_arc,
            is_remote: false,
            session_name: config.session_name.clone(),
            slow_mo_ms: config.slow_mo_ms,
        })
    }

    pub fn is_remote(&self) -> bool {
        self.is_remote
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub async fn debug_ws_url(&self) -> Option<String> {
        let lock = self.browser.lock().await;
        lock.as_ref().map(|b| b.websocket_address().clone())
    }

    pub async fn pace(&self) {
        if let Some(ms) = self.slow_mo_ms {
            if ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
            }
        }
    }

    pub async fn new_page(&self, url: &str) -> Result<Page, AppError> {
        let lock = self.browser.lock().await;
        let b = lock.as_ref().ok_or_else(|| AppError::Browser("Browser session is closed".to_string()))?;
        let page = b.new_page(url).await.map_err(|e| AppError::Browser(format!("Failed to open page: {e}")))?;
        crate::browser::stealth::apply_stealth(&page).await?;
        self.pace().await;
        Ok(page)
    }

    pub async fn close(&self) -> Result<(), AppError> {
        if !self.is_remote {
            let mut lock = self.browser.lock().await;
            if let Some(mut b) = lock.take() {
                debug!(target: "browser", "Closing Chromium instance cleanly via CDP...");
                let _ = b.close().await;
                let _ = b.wait().await;
            }
        }
        Ok(())
    }
}

fn resolve_chromium_executable() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("CHROMIUM_PATH").or_else(|_| std::env::var("CHROME_PATH")) {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    let candidates = [
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/opt/homebrew/bin/chromium",
        "/usr/local/bin/chromium",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];

    for candidate in candidates {
        let p = std::path::PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg("chromium").output() {
        if output.status.success() {
            let p_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let p = std::path::PathBuf::from(p_str);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}
