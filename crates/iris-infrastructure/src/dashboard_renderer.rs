use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use iris_core::{IrisError, Result, ScreenType};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

const DEFAULT_BROWSERLESS_URL: &str = "http://browserless-chrome.browserless-chrome.svc/function";
const DEFAULT_GRAFANA_INTERNAL_URL: &str = "http://grafana-service.grafana.svc:3000";
const DEFAULT_OPENOBSERVE_INTERNAL_URL: &str = "http://o2-openobserve-router.openobserve.svc:5080";
const DEFAULT_GRAFANA_USER: &str = "admin";
const DEFAULT_GRAFANA_PASSWORD: &str = "Superman69230";
const DEFAULT_OPENOBSERVE_EMAIL: &str = "joseph@zacharie.org";
const DEFAULT_OPENOBSERVE_PASSWORD: &str = "Superman69230";

#[derive(Debug, Clone)]
pub struct DashboardRendererConfig {
    pub browserless_url: String,
    pub grafana_internal_url: String,
    pub grafana_user: String,
    pub grafana_password: String,
    pub openobserve_internal_url: String,
    pub openobserve_email: String,
    pub openobserve_password: String,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub timeout_seconds: u64,
}

impl Default for DashboardRendererConfig {
    fn default() -> Self {
        Self {
            browserless_url: DEFAULT_BROWSERLESS_URL.to_string(),
            grafana_internal_url: DEFAULT_GRAFANA_INTERNAL_URL.to_string(),
            grafana_user: DEFAULT_GRAFANA_USER.to_string(),
            grafana_password: DEFAULT_GRAFANA_PASSWORD.to_string(),
            openobserve_internal_url: DEFAULT_OPENOBSERVE_INTERNAL_URL.to_string(),
            openobserve_email: DEFAULT_OPENOBSERVE_EMAIL.to_string(),
            openobserve_password: DEFAULT_OPENOBSERVE_PASSWORD.to_string(),
            viewport_width: 1920,
            viewport_height: 1080,
            timeout_seconds: 60,
        }
    }
}

impl DashboardRendererConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = env::var("IRIS_BROWSERLESS_URL") {
            if !v.trim().is_empty() {
                cfg.browserless_url = if v.ends_with("/function") {
                    v
                } else {
                    format!("{}/function", v.trim_end_matches('/'))
                };
            }
        }
        if let Ok(v) = env::var("GRAFANA_INTERNAL_URL") {
            if !v.trim().is_empty() {
                cfg.grafana_internal_url = v.trim_end_matches('/').to_string();
            }
        }
        if let Ok(v) = env::var("GRAFANA_ADMIN_USER") {
            if !v.trim().is_empty() {
                cfg.grafana_user = v;
            }
        }
        if let Ok(v) = env::var("GRAFANA_ADMIN_PASSWORD") {
            if !v.trim().is_empty() {
                cfg.grafana_password = v;
            }
        }
        if let Ok(v) = env::var("OPENOBSERVE_INTERNAL_URL") {
            if !v.trim().is_empty() {
                cfg.openobserve_internal_url = v.trim_end_matches('/').to_string();
            }
        }
        if let Ok(v) = env::var("OPENOBSERVE_EMAIL") {
            if !v.trim().is_empty() {
                cfg.openobserve_email = v;
            }
        }
        if let Ok(v) = env::var("OPENOBSERVE_PASSWORD") {
            if !v.trim().is_empty() {
                cfg.openobserve_password = v;
            }
        }
        cfg
    }
}

#[derive(Debug, Clone)]
pub struct RenderCacheEntry {
    pub image_png: Vec<u8>,
    pub rendered_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub target_url: String,
}

pub struct DashboardRenderer {
    config: DashboardRendererConfig,
    http: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, RenderCacheEntry>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
}

impl DashboardRenderer {
    pub fn new(config: DashboardRendererConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config,
            http,
            cache: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn from_env() -> Self {
        Self::new(DashboardRendererConfig::from_env())
    }

    pub fn config(&self) -> &DashboardRendererConfig {
        &self.config
    }

    pub async fn get_cached(&self, screen_id: &str) -> Option<RenderCacheEntry> {
        let lock = self.cache.read().await;
        lock.get(screen_id).cloned()
    }

    pub async fn get_cached_for_url(
        &self,
        screen_id: &str,
        expected_url: &str,
    ) -> Option<RenderCacheEntry> {
        let lock = self.cache.read().await;
        let entry = lock.get(screen_id)?;
        if expected_url.trim().is_empty() || entry.target_url == expected_url.trim() {
            Some(entry.clone())
        } else {
            None
        }
    }

    pub async fn render_and_cache(
        &self,
        screen_id: &str,
        screen_type: ScreenType,
        target_url: &str,
    ) -> Result<RenderCacheEntry> {
        let normalized_target = target_url.trim().to_string();
        let flight_key = format!("{screen_id}|{normalized_target}");

        // Single-flight deduplication: prevent duplicate concurrent renders for the same screen+url
        loop {
            let is_busy = {
                let mut guard = self.in_flight.lock().await;
                if guard.contains(&flight_key) {
                    true
                } else {
                    guard.insert(flight_key.clone());
                    false
                }
            };

            if !is_busy {
                break;
            }

            // A render for this exact screen+url is already running, await it
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(cached) = self.get_cached_for_url(screen_id, &normalized_target).await {
                if (Utc::now() - cached.rendered_at).num_seconds() < 15 {
                    return Ok(cached);
                }
            }
        }

        let start = Instant::now();
        info!(
            screen_id = %screen_id,
            screen_type = ?screen_type,
            url = %normalized_target,
            "Starting dashboard pre-render via browserless-chrome"
        );

        let render_res = self.render_screen(screen_type, &normalized_target).await;

        // Release in-flight flag
        {
            let mut guard = self.in_flight.lock().await;
            guard.remove(&flight_key);
        }

        let image_png = render_res?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let entry = RenderCacheEntry {
            image_png,
            rendered_at: Utc::now(),
            duration_ms,
            target_url: normalized_target,
        };

        {
            let mut lock = self.cache.write().await;
            lock.insert(screen_id.to_string(), entry.clone());
        }

        info!(
            screen_id = %screen_id,
            duration_ms = duration_ms,
            bytes = entry.image_png.len(),
            "Dashboard pre-render completed and cached successfully"
        );
        Ok(entry)
    }

    pub async fn render_screen(&self, screen_type: ScreenType, raw_url: &str) -> Result<Vec<u8>> {
        let script = self.build_puppeteer_script(screen_type, raw_url)?;
        let endpoint = if self.config.browserless_url.contains("timeout=") {
            self.config.browserless_url.clone()
        } else {
            let sep = if self.config.browserless_url.contains('?') {
                '&'
            } else {
                '?'
            };
            format!("{}{sep}timeout=55000", self.config.browserless_url)
        };
        debug!(
            browserless_url = %endpoint,
            "Sending puppeteer script to browserless"
        );

        let response = self
            .http
            .post(&endpoint)
            .header("Content-Type", "application/javascript")
            .body(script)
            .send()
            .await
            .map_err(|e| {
                IrisError::Render(format!(
                    "failed to communicate with browserless at {}: {e}",
                    endpoint
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(IrisError::Render(format!(
                "browserless returned HTTP {status}: {error_text}"
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| IrisError::Render(format!("failed to read render bytes: {e}")))?;

        let vec_bytes = bytes.to_vec();
        // Verify PNG signature: 89 50 4E 47 0D 0A 1A 0A
        if vec_bytes.len() < 8 || &vec_bytes[..4] != b"\x89PNG" {
            let sample = String::from_utf8_lossy(&vec_bytes[..vec_bytes.len().min(200)]);
            warn!(
                sample = %sample,
                "Rendered output does not have standard PNG header"
            );
            return Err(IrisError::Render(format!(
                "invalid PNG payload from browserless: {sample}"
            )));
        }

        Ok(vec_bytes)
    }

    fn build_puppeteer_script(&self, screen_type: ScreenType, raw_url: &str) -> Result<String> {
        let width = self.config.viewport_width;
        let height = self.config.viewport_height;

        match screen_type {
            ScreenType::Grafana => {
                let internal_url = self.format_internal_grafana_url(raw_url);
                let auth_token = format!(
                    "{}:{}",
                    self.config.grafana_user, self.config.grafana_password
                );
                let auth_b64 = simple_base64_encode(&auth_token);

                let script = format!(
                    r#"module.exports = async ({{ page }}) => {{
  let pendingQueries = 0;
  let totalQueriesFired = 0;

  page.on('request', req => {{
    if (req.url().includes('/api/ds/query')) {{
      pendingQueries++;
      totalQueriesFired++;
    }}
  }});

  page.on('requestfinished', req => {{
    if (req.url().includes('/api/ds/query')) {{
      pendingQueries = Math.max(0, pendingQueries - 1);
    }}
  }});

  page.on('requestfailed', req => {{
    if (req.url().includes('/api/ds/query')) {{
      pendingQueries = Math.max(0, pendingQueries - 1);
    }}
  }});

  await page.setViewport({{ width: {width}, height: {height}, deviceScaleFactor: 1 }});
  await page.setExtraHTTPHeaders({{
    'Authorization': 'Basic {auth_b64}'
  }});
  await page.goto('{internal_url}', {{
    waitUntil: 'domcontentloaded',
    timeout: 35000
  }});

  // 1. Wait up to 6s for dashboard queries to be dispatched by Grafana
  for (let i = 0; i < 60; i++) {{
    if (totalQueriesFired > 0) break;
    await new Promise(r => setTimeout(r, 100));
  }}

  // 2. Wait up to 25s for all in-flight Prometheus queries to complete
  for (let i = 0; i < 125; i++) {{
    if (totalQueriesFired > 0 && pendingQueries === 0) break;
    await new Promise(r => setTimeout(r, 200));
  }}

  // 3. Ensure no remaining panel loading spinners, plugin chunks, or cancel buttons exist
  try {{
    await page.waitForFunction(() => {{
      const cancelBtn = Array.from(document.querySelectorAll('button, span')).some(el => el.textContent && el.textContent.includes('Cancel'));
      const spinners = document.querySelectorAll('.panel-loading, [data-testid="panel-loading"], .fa-spinner');
      const loadingPlugin = document.body && document.body.innerText && document.body.innerText.includes('Loading plugin panel');
      return !cancelBtn && spinners.length === 0 && !loadingPlugin;
    }}, {{ timeout: 8000 }});
  }} catch (e) {{}}

  // 4. Inject clean kiosk CSS to remove top toolbars, variable submenus, and scrollbars
  try {{
    await page.addStyleTag({{
      content: `
        header, [data-testid="navbarmenu"], [aria-label="Dashboard submenu"], .submenu-controls,
        [data-testid="data-testid dashboard controls"], [role="alert"] {{
          display: none !important;
        }}
        ::-webkit-scrollbar {{
          display: none !important;
        }}
      `
    }});
  }} catch (e) {{}}

  // 5. Wait for uPlot/canvas drawing to settle
  await new Promise(r => setTimeout(r, 2500));

  const buffer = await page.screenshot({{
    type: 'png',
    fullPage: false
  }});
  return {{
    data: buffer,
    type: 'image/png'
  }};
}};"#
                );
                Ok(script)
            }
            ScreenType::OpenObserve => {
                let internal_url = self.format_internal_openobserve_url(raw_url);
                let login_url = format!("{}/web/login", self.config.openobserve_internal_url);
                let email = &self.config.openobserve_email;
                let password = &self.config.openobserve_password;

                let script = format!(
                    r#"module.exports = async ({{ page }}) => {{
  let pendingO2Queries = 0;
  let o2QueriesFired = 0;

  page.on('request', req => {{
    if (req.url().includes('/api/default/')) {{
      pendingO2Queries++;
      o2QueriesFired++;
    }}
  }});

  page.on('requestfinished', req => {{
    if (req.url().includes('/api/default/')) {{
      pendingO2Queries = Math.max(0, pendingO2Queries - 1);
    }}
  }});

  page.on('requestfailed', req => {{
    if (req.url().includes('/api/default/')) {{
      pendingO2Queries = Math.max(0, pendingO2Queries - 1);
    }}
  }});

  await page.setViewport({{ width: {width}, height: {height}, deviceScaleFactor: 1 }});
  await page.goto('{login_url}', {{
    waitUntil: 'domcontentloaded',
    timeout: 30000
  }});
  await new Promise(r => setTimeout(r, 1500));
  await page.evaluate((email, pass) => {{
    const inputs = document.querySelectorAll('input');
    for (const inp of inputs) {{
      if (inp.type === 'text' || inp.type === 'email' || (inp.name && inp.name.includes('email'))) {{
        inp.value = email;
        inp.dispatchEvent(new Event('input', {{ bubbles: true }}));
      }} else if (inp.type === 'password') {{
        inp.value = pass;
        inp.dispatchEvent(new Event('input', {{ bubbles: true }}));
      }}
    }}
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {{
      if (btn.innerText && (btn.innerText.includes('Sign In') || btn.innerText.includes('Se connecter') || btn.innerText.includes('Login'))) {{
        btn.click();
        break;
      }}
    }}
  }}, '{email}', '{password}');

  await new Promise(r => setTimeout(r, 2000));
  await page.evaluate((url) => {{
    window.location.href = url;
  }}, '{internal_url}');

  // Wait up to 5s for queries to start
  for (let i = 0; i < 50; i++) {{
    if (o2QueriesFired > 0) break;
    await new Promise(r => setTimeout(r, 100));
  }}

  // Wait up to 20s for queries to finish
  for (let i = 0; i < 100; i++) {{
    if (o2QueriesFired > 0 && pendingO2Queries === 0) break;
    await new Promise(r => setTimeout(r, 200));
  }}

  try {{
    await page.addStyleTag({{
      content: `
        .q-notifications, [role="alert"] {{
          display: none !important;
        }}
        ::-webkit-scrollbar {{
          display: none !important;
        }}
      `
    }});
  }} catch (e) {{}}

  await new Promise(r => setTimeout(r, 2500));
  const buffer = await page.screenshot({{
    type: 'png',
    fullPage: false
  }});
  return {{
    data: buffer,
    type: 'image/png'
  }};
}};"#
                );
                Ok(script)
            }
            ScreenType::Web => {
                let trimmed = raw_url.trim();
                let url = if trimmed.is_empty() {
                    "https://iris.zacharie.org"
                } else {
                    trimmed
                };
                let script = format!(
                    r#"module.exports = async ({{ page }}) => {{
  await page.setViewport({{ width: {width}, height: {height}, deviceScaleFactor: 1 }});
  await page.goto('{url}', {{
    waitUntil: 'domcontentloaded',
    timeout: 30000
  }});
  await new Promise(r => setTimeout(r, 3000));
  const buffer = await page.screenshot({{
    type: 'png',
    fullPage: false
  }});
  return {{
    data: buffer,
    type: 'image/png'
  }};
}};"#
                );
                Ok(script)
            }
            _ => Err(IrisError::Render(format!(
                "ScreenType {:?} is not a browser-renderable dashboard",
                screen_type
            ))),
        }
    }

    fn format_internal_grafana_url(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        let base_path = if trimmed.is_empty() {
            "/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard"
        } else if let Some(idx) = trimmed.find("/d/") {
            &trimmed[idx..]
        } else if trimmed.starts_with('/') {
            trimmed
        } else {
            "/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard"
        };

        let mut url = format!("{}{}", self.config.grafana_internal_url, base_path);
        if url.contains("kiosk=tv") {
            url = url.replace("kiosk=tv", "kiosk=1");
        } else if !url.contains("kiosk") {
            let sep = if url.contains('?') { '&' } else { '?' };
            url = format!("{url}{sep}kiosk=1");
        }
        if !url.contains("theme=") {
            let sep = if url.contains('?') { '&' } else { '?' };
            url = format!("{url}{sep}theme=dark");
        }
        url
    }

    fn format_internal_openobserve_url(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        // If raw is just an ID (e.g. numeric "7516898066293608448" or slug)
        if !trimmed.is_empty() && !trimmed.contains('/') {
            return format!(
                "{}/web/dashboards/view?org_identifier=default&dashboard={}&folder=default&hide_header=true",
                self.config.openobserve_internal_url, trimmed
            );
        }

        let base_path = if trimmed.is_empty()
            || trimmed.ends_with("/dashboards")
            || trimmed.ends_with("/dashboards/")
            || trimmed.contains("k8s-etcd-objects-inventory")
        {
            "/web/dashboards/view?org_identifier=default&dashboard=7516898066293608448&folder=default"
        } else if let Some(idx) = trimmed.find("/web/") {
            &trimmed[idx..]
        } else if trimmed.starts_with('/') {
            trimmed
        } else {
            "/web/dashboards/view?org_identifier=default&dashboard=7516898066293608448&folder=default"
        };

        let mut url = format!("{}{}", self.config.openobserve_internal_url, base_path);
        if url.contains("dashboard_id=") {
            url = url.replace("dashboard_id=", "dashboard=");
        }
        if !url.contains("folder=") {
            let sep = if url.contains('?') { '&' } else { '?' };
            url = format!("{url}{sep}folder=default");
        }
        if !url.contains("hide_header") {
            let sep = if url.contains('?') { '&' } else { '?' };
            url = format!("{url}{sep}hide_header=true");
        }
        url
    }

    pub fn spawn_background_refresher(
        renderer: Arc<Self>,
        controller: Arc<iris_application::IrisController>,
        interval: Duration,
    ) {
        tokio::spawn(async move {
            info!(
                interval_secs = interval.as_secs(),
                "Started Iris dashboard background pre-rendering loop"
            );
            // Initial render delay to let services stabilize
            tokio::time::sleep(Duration::from_secs(3)).await;

            loop {
                let snap = controller.snapshot().await;
                let displayed_id = snap.displayed_screen.as_ref().map(|s| s.id.clone());

                let mut targets: Vec<_> = snap
                    .screens
                    .into_iter()
                    .filter(|s| {
                        s.enabled
                            && matches!(
                                s.screen_type,
                                ScreenType::Grafana | ScreenType::OpenObserve
                            )
                    })
                    .collect();

                // Always prioritize the currently displayed screen first
                targets.sort_by_key(|s| Some(s.id.clone()) != displayed_id);

                for screen in targets {
                    let target_url = if screen.url.trim().is_empty() {
                        &snap.formatted_url
                    } else {
                        &screen.url
                    };
                    match renderer
                        .render_and_cache(&screen.id, screen.screen_type, target_url)
                        .await
                    {
                        Ok(_) => {
                            debug!(screen = %screen.name, "Background pre-render updated");
                        }
                        Err(e) => {
                            warn!(
                                screen = %screen.name,
                                error = %e,
                                "Background pre-render failed"
                            );
                        }
                    }
                }

                tokio::time::sleep(interval).await;
            }
        });
    }

    pub fn generate_fallback_svg(title: &str, error_msg: &str) -> Vec<u8> {
        let escaped_title = escape_xml(title);
        let escaped_err = escape_xml(error_msg);
        let now = Utc::now().format("%H:%M:%S UTC").to_string();

        let mut svg = String::with_capacity(1024);
        svg.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\" viewBox=\"0 0 1920 1080\">");
        svg.push_str("<defs><linearGradient id=\"bg\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">");
        svg.push_str("<stop offset=\"0%\" stop-color=\"#090d16\"/>");
        svg.push_str("<stop offset=\"100%\" stop-color=\"#111827\"/>");
        svg.push_str("</linearGradient></defs>");
        svg.push_str("<rect width=\"1920\" height=\"1080\" fill=\"url(#bg)\"/>");
        svg.push_str("<rect x=\"160\" y=\"240\" width=\"1600\" height=\"600\" rx=\"24\" fill=\"#1e293b\" stroke=\"#334155\" stroke-width=\"2\"/>");
        svg.push_str("<text x=\"960\" y=\"380\" font-family=\"sans-serif\" font-size=\"44\" font-weight=\"700\" fill=\"#38bdf8\" text-anchor=\"middle\">IRIS · Pré-rendu K8s en cours</text>");
        svg.push_str(&format!("<text x=\"960\" y=\"460\" font-family=\"sans-serif\" font-size=\"32\" font-weight=\"600\" fill=\"#f1f5f9\" text-anchor=\"middle\">{escaped_title}</text>"));
        svg.push_str("<text x=\"960\" y=\"550\" font-family=\"sans-serif\" font-size=\"20\" fill=\"#94a3b8\" text-anchor=\"middle\">Génération par Chrome headless (browserless-chrome.browserless-chrome.svc)</text>");
        svg.push_str("<rect x=\"360\" y=\"600\" width=\"1200\" height=\"90\" rx=\"12\" fill=\"#090d16\" stroke=\"#475569\" stroke-width=\"1\"/>");
        svg.push_str(&format!("<text x=\"960\" y=\"655\" font-family=\"monospace\" font-size=\"18\" fill=\"#f59e0b\" text-anchor=\"middle\">{escaped_err}</text>"));
        svg.push_str(&format!("<text x=\"960\" y=\"770\" font-family=\"sans-serif\" font-size=\"16\" fill=\"#64748b\" text-anchor=\"middle\">Horodatage: {now} · Rafraîchissement automatique actif</text>"));
        svg.push_str("</svg>");

        svg.into_bytes()
    }
}

fn simple_base64_encode(input: &str) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encoding_works() {
        assert_eq!(
            simple_base64_encode("admin:Superman69230"),
            "YWRtaW46U3VwZXJtYW42OTIzMA=="
        );
        assert_eq!(simple_base64_encode("hello"), "aGVsbG8=");
    }

    #[test]
    fn format_grafana_internal_url() {
        let r = DashboardRenderer::new(DashboardRendererConfig::default());
        let url = r.format_internal_grafana_url(
            "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard",
        );
        assert!(url.contains("http://grafana-service.grafana.svc:3000/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard"));
        assert!(url.contains("kiosk=1"));
        assert!(url.contains("theme=dark"));
    }

    #[test]
    fn format_openobserve_internal_url() {
        let r = DashboardRenderer::new(DashboardRendererConfig::default());
        let url = r.format_internal_openobserve_url("7526637916655718400");
        assert_eq!(
            url,
            "http://o2-openobserve-router.openobserve.svc:5080/web/dashboards/view?org_identifier=default&dashboard=7526637916655718400&folder=default&hide_header=true"
        );
    }
}
