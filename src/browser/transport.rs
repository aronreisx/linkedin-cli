use crate::errors::AppError;
use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::debug;

pub struct InPageTransport;

impl InPageTransport {
    /// Execute an in-browser fetch() call, inheriting all cookies, TLS fingerprint, and browser session context.
    pub async fn fetch(
        page: &Page,
        method: &str,
        url: &str,
        custom_headers: &[(&str, &str)],
        body: Option<&str>,
        timeout_ms: u64,
    ) -> Result<(u16, bool, String), AppError> {
        debug!(target: "transport", "Executing in-page fetch {method} {url}");

        let mut headers_map = serde_json::Map::new();
        for (k, v) in custom_headers {
            headers_map.insert((*k).to_string(), Value::String((*v).to_string()));
        }

        let args = json!([method, url, Value::Object(headers_map), body, timeout_ms]);

        let args_json = serde_json::to_string(&args).map_err(AppError::Json)?;

        let script = format!(
            r#"
            (async ([method, url, headersObj, bodyStr, timeoutMs]) => {{
                const getCookie = (name) => {{
                    const match = document.cookie.match(new RegExp('(^|;\\s*)(' + name + ')=([^;]*)'));
                    return match ? decodeURIComponent(match[3]) : null;
                }};

                let js_session_id = getCookie('JSESSIONID');
                if (js_session_id && js_session_id.startsWith('"') && js_session_id.endsWith('"')) {{
                    js_session_id = js_session_id.slice(1, -1);
                }}

                const headers = {{
                    'accept': 'application/vnd.linkedin.normalized+json+2.1',
                    'x-restli-protocol-version': '2.0.0',
                    'x-li-lang': 'en_US',
                    ...headersObj
                }};

                if (js_session_id && !headers['csrf-token']) {{
                    headers['csrf-token'] = js_session_id;
                }}

                const controller = new AbortController();
                const timer = setTimeout(() => controller.abort(), timeoutMs);

                const init = {{
                    method: method,
                    headers: headers,
                    credentials: 'include',
                    redirect: 'manual',
                    signal: controller.signal
                }};

                if (bodyStr !== null && bodyStr !== undefined) {{
                    init.body = bodyStr;
                }}

                try {{
                    const response = await fetch(url, init);
                    clearTimeout(timer);
                    const text = await response.text();
                    return {{
                        status: response.status,
                        ok: response.ok,
                        body: text
                    }};
                }} catch (err) {{
                    clearTimeout(timer);
                    return {{
                        status: 0,
                        ok: false,
                        body: String(err)
                    }};
                }}
            }})({args_json})
        "#
        );

        let eval_res = page
            .evaluate(script)
            .await
            .map_err(|e| AppError::Cdp(format!("In-page fetch evaluation failed: {e}")))?;

        let res_val = eval_res
            .into_value::<Value>()
            .map_err(|e| AppError::Cdp(format!("Failed to parse fetch response: {e}")))?;

        let status = res_val["status"].as_u64().unwrap_or(0) as u16;
        let ok = res_val["ok"].as_bool().unwrap_or(false);
        let text = res_val["body"].as_str().unwrap_or("").to_string();

        if status == 0 && !ok {
            return Err(AppError::NavigationFailed {
                url: url.to_string(),
                reason: text,
            });
        }

        Ok((status, ok, text))
    }
}
