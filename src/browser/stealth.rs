use chromiumoxide::Page;
use tracing::debug;
use crate::errors::AppError;

/// Stealth JavaScript payload to mask headless Chromium detection vectors.
/// Based on Playwright-Stealth / Puppeteer-Extra-Stealth evasions.
pub const STEALTH_JS_PAYLOAD: &str = r#"
(() => {
    // 1. Mask navigator.webdriver
    try {
        Object.defineProperty(Object.getPrototypeOf(navigator), 'webdriver', {
            get: () => undefined,
            enumerable: true,
            configurable: true,
        });
        delete navigator.webdriver;
    } catch (_) {}

    // 2. Mock window.chrome object
    try {
        if (!window.chrome) {
            window.chrome = {
                app: {
                    isInstalled: false,
                    InstallState: { DISABLED: 'DISABLED', INSTALLED: 'INSTALLED', NOT_INSTALLED: 'NOT_INSTALLED' },
                    RunningState: { CANNOT_RUN: 'CANNOT_RUN', READY_TO_RUN: 'READY_TO_RUN', RUNNING: 'RUNNING' }
                },
                runtime: {
                    OnInstalledReason: { CHROME_UPDATE: 'chrome_update', INSTALL: 'install', SHARED_MODULE_UPDATE: 'shared_module_update', UPDATE: 'update' },
                    OnRestartRequiredReason: { APP_UPDATE: 'app_update', OS_UPDATE: 'os_update', PERIODIC: 'periodic' },
                    PlatformArch: { ARM: 'arm', ARM64: 'arm64', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
                    PlatformNaclArch: { ARM: 'arm', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
                    PlatformOs: { ANDROID: 'android', CROS: 'cros', LINUX: 'linux', MAC: 'mac', OPENBSD: 'openbsd', WIN: 'win' },
                    RequestUpdateCheckStatus: { NO_UPDATE: 'no_update', THROTTLED: 'throttled', UPDATE_AVAILABLE: 'update_available' }
                },
                loadTimes: function() {
                    const now = Date.now() / 1000;
                    return {
                        commitLoadTime: now - 0.5,
                        connectionInfo: 'h2',
                        finishDocumentLoadTime: now - 0.1,
                        finishLoadTime: now,
                        firstPaintAfterLoadTime: 0,
                        firstPaintTime: now - 0.4,
                        navigationType: 'Other',
                        npnNegotiatedProtocol: 'h2',
                        requestTime: now - 0.6,
                        startLoadTime: now - 0.6,
                        wasAlternateProtocolAvailable: false,
                        wasFetchedViaSpdy: true,
                        wasNpnNegotiated: true
                    };
                },
                csi: function() {
                    return {
                        startE: Date.now() - 500,
                        onloadT: Date.now(),
                        pageT: 500,
                        tran: 15
                    };
                }
            };
        }
    } catch (_) {}

    // 3. Spoof WebGL vendor and renderer (hide SwiftShader/Mesa headless software rasterizers)
    try {
        const patchWebGL = (proto) => {
            if (!proto || !proto.getParameter) return;
            const originalGetParameter = proto.getParameter;
            proto.getParameter = function(param) {
                // UNMASKED_VENDOR_WEBGL
                if (param === 37445) {
                    return 'Intel Inc.';
                }
                // UNMASKED_RENDERER_WEBGL
                if (param === 37446) {
                    return 'Intel Iris OpenGL Engine';
                }
                return originalGetParameter.apply(this, arguments);
            };
        };
        if (typeof WebGLRenderingContext !== 'undefined') patchWebGL(WebGLRenderingContext.prototype);
        if (typeof WebGL2RenderingContext !== 'undefined') patchWebGL(WebGL2RenderingContext.prototype);
    } catch (_) {}

    // 4. Mock navigator.plugins & navigator.mimeTypes
    try {
        if (!navigator.plugins || navigator.plugins.length === 0) {
            const makePlugin = (name, filename, description) => {
                const p = Object.create(Plugin.prototype || {});
                Object.defineProperties(p, {
                    name: { value: name, enumerable: true },
                    filename: { value: filename, enumerable: true },
                    description: { value: description, enumerable: true },
                    length: { value: 0, enumerable: true }
                });
                return p;
            };
            const plugins = [
                makePlugin('PDF Viewer', 'pdf-viewer.plugin', 'Portable Document Format'),
                makePlugin('Chrome PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
                makePlugin('Chromium PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format')
            ];
            Object.defineProperty(navigator, 'plugins', {
                get: () => plugins,
                enumerable: true,
                configurable: true
            });
        }
    } catch (_) {}

    // 5. Mock navigator.languages
    try {
        Object.defineProperty(navigator, 'languages', {
            get: () => ['en-US', 'en'],
            enumerable: true,
            configurable: true
        });
    } catch (_) {}

    // 6. Mock Notification permissions query
    try {
        if (navigator.permissions && navigator.permissions.query) {
            const originalQuery = navigator.permissions.query;
            navigator.permissions.query = function(parameters) {
                if (parameters && parameters.name === 'notifications') {
                    return Promise.resolve({
                        state: typeof Notification !== 'undefined' && Notification.permission === 'denied' ? 'denied' : 'prompt',
                        onchange: null
                    });
                }
                return originalQuery.apply(this, arguments);
            };
        }
    } catch (_) {}
})();
"#;

/// Applies anti-detection stealth scripts to a Chromium page instance.
/// 
/// Uses `Page.addScriptToEvaluateOnNewDocument` so any future navigation within the
/// page automatically runs the stealth shims prior to execution of any site scripts.
/// Also evaluates the script immediately in the currently loaded document.
pub async fn apply_stealth(page: &Page) -> Result<(), AppError> {
    debug!(target: "browser", "Applying CDP stealth script on new document...");
    page.evaluate_on_new_document(STEALTH_JS_PAYLOAD)
        .await
        .map_err(|e| AppError::Browser(format!("Failed to register stealth on new document: {e}")))?;

    // Also evaluate in currently active document if one exists
    let _ = page.evaluate(STEALTH_JS_PAYLOAD).await;

    Ok(())
}
