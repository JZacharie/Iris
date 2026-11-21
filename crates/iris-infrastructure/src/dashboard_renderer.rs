use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use iris_core::{IrisError, Result, ScreenType};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, info, warn};

const DEFAULT_BROWSERLESS_URL: &str = "http://browserless-chrome.browserless-chrome.svc/function";
const DEFAULT_GRAFANA_INTERNAL_URL: &str = "http://grafana-service.grafana.svc:3000";
const DEFAULT_OPENOBSERVE_INTERNAL_URL: &str = "http://o2-openobserve-router.openobserve.svc:5080";
const DEFAULT_GRAFANA_USER: &str = "admin";
const DEFAULT_GRAFANA_PASSWORD: &str = "Superman69230";
const DEFAULT_OPENOBSERVE_EMAIL: &str = "joseph@zacharie.org";
const DEFAULT_OPENOBSERVE_PASSWORD: &str = "Superman69230";

pub const CURATED_GRAFANA_URLS: &[&str] = &[
    "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard",
    "https://grafana.zacharie.org/d/rYdddlPWk/node-exporter-full",
    "https://grafana.zacharie.org/d/qPkgGHg7k/argocd",
    "https://grafana.zacharie.org/d/cloudnative-pg/cloudnativepg",
    "https://grafana.zacharie.org/d/wY4blRMGz/coredns",
    "https://grafana.zacharie.org/d/tjRlQw6Zk/emqx-monitoring",
    "https://grafana.zacharie.org/d/Kn5xm-gZk/rabbitmq-overview",
    "https://grafana.zacharie.org/d/vaults/hashicorp-vault",
    "https://grafana.zacharie.org/d/cilium-metrics/cilium-metrics",
];

pub const CURATED_OPENOBSERVE_IDS: &[&str] = &[
    "7516898066293608448",
    "7516898328903180289",
    "7516531994134597633",
    "7516513601809571840",
    "7526637916655718400",
    "7502190376086695936",
    "7502190373440069632",
    "7502190357904367616",
    "7502190361909927936",
    "7502197767322759168",
];

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
            viewport_width: 1528,
            viewport_height: 1122,
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
    pub width: u32,
    pub height: u32,
}

pub struct DashboardRenderer {
    config: DashboardRendererConfig,
    http: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, RenderCacheEntry>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    semaphore: Arc<Semaphore>,
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
            semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    pub fn from_env() -> Self {
        Self::new(DashboardRendererConfig::from_env())
    }

    pub fn config(&self) -> &DashboardRendererConfig {
        &self.config
    }

    pub fn normalize_dimensions(&self, w: Option<u32>, h: Option<u32>) -> (u32, u32) {
        let width = w.unwrap_or(self.config.viewport_width).clamp(640, 3840);
        let height = h.unwrap_or(self.config.viewport_height).clamp(480, 2160);
        (width, height)
    }

    pub fn canonical_target_key(&self, screen_type: ScreenType, raw_url: &str) -> String {
        match screen_type {
            ScreenType::Grafana => self.format_internal_grafana_url(raw_url),
            ScreenType::OpenObserve => self.format_internal_openobserve_url(raw_url),
            _ => raw_url.trim().to_string(),
        }
    }

    fn cache_key(target_key: &str, width: u32, height: u32) -> String {
        format!("{target_key}@{width}x{height}")
    }

    pub async fn get_cached(&self, screen_id: &str) -> Option<RenderCacheEntry> {
        let lock = self.cache.read().await;
        let default_key = Self::cache_key(
            screen_id,
            self.config.viewport_width,
            self.config.viewport_height,
        );
        if let Some(entry) = lock.get(&default_key) {
            return Some(entry.clone());
        }
        let prefix = format!("{screen_id}@");
        lock.iter()
            .filter(|(k, _)| k.starts_with(&prefix) || *k == screen_id)
            .max_by_key(|(_, v)| v.rendered_at)
            .map(|(_, v)| v.clone())
    }

    pub async fn get_cached_for_url(
        &self,
        screen_id: &str,
        expected_url: &str,
    ) -> Option<RenderCacheEntry> {
        self.get_cached_for_url_and_size(screen_id, expected_url, None, None)
            .await
    }

    pub async fn get_cached_for_url_and_size(
        &self,
        screen_id: &str,
        expected_url: &str,
        w: Option<u32>,
        h: Option<u32>,
    ) -> Option<RenderCacheEntry> {
        let (width, height) = self.normalize_dimensions(w, h);
        let expected_trimmed = expected_url.trim();
        let lock = self.cache.read().await;

        if !expected_trimmed.is_empty() {
            let exact_key = Self::cache_key(expected_trimmed, width, height);
            if let Some(entry) = lock.get(&exact_key) {
                return Some(entry.clone());
            }
            if let Some(entry) = lock
                .values()
                .filter(|v| v.target_url == expected_trimmed)
                .max_by_key(|v| (v.width == width && v.height == height, v.rendered_at))
            {
                return Some(entry.clone());
            }
        }

        let key = Self::cache_key(screen_id, width, height);
        if let Some(entry) = lock.get(&key) {
            if expected_trimmed.is_empty() || entry.target_url == expected_trimmed {
                return Some(entry.clone());
            }
        }
        None
    }

    pub async fn get_cached_for_screen(
        &self,
        screen_id: &str,
        screen_type: ScreenType,
        expected_url: &str,
        w: Option<u32>,
        h: Option<u32>,
    ) -> Option<RenderCacheEntry> {
        let (width, height) = self.normalize_dimensions(w, h);
        let canonical = self.canonical_target_key(screen_type, expected_url);
        let lock = self.cache.read().await;

        let exact_key = Self::cache_key(&canonical, width, height);
        if let Some(entry) = lock.get(&exact_key) {
            return Some(entry.clone());
        }

        // Fall back to another resolution of the exact same canonical dashboard URL
        if let Some(entry) = lock
            .values()
            .filter(|v| v.target_url == canonical)
            .max_by_key(|v| (v.width == width && v.height == height, v.rendered_at))
        {
            return Some(entry.clone());
        }

        if expected_url.trim().is_empty() {
            let screen_key = Self::cache_key(screen_id, width, height);
            return lock.get(&screen_key).cloned();
        }

        None
    }

    pub async fn is_fresh(
        &self,
        screen_type: ScreenType,
        target_url: &str,
        width: u32,
        height: u32,
        max_age_secs: i64,
    ) -> bool {
        let canonical = self.canonical_target_key(screen_type, target_url);
        let key = Self::cache_key(&canonical, width, height);
        let lock = self.cache.read().await;
        if let Some(entry) = lock.get(&key) {
            return (Utc::now() - entry.rendered_at).num_seconds() < max_age_secs;
        }
        false
    }

    pub async fn render_and_cache(
        &self,
        screen_id: &str,
        screen_type: ScreenType,
        target_url: &str,
    ) -> Result<RenderCacheEntry> {
        self.render_and_cache_sized(screen_id, screen_type, target_url, None, None)
            .await
    }

    pub async fn render_and_cache_sized(
        &self,
        screen_id: &str,
        screen_type: ScreenType,
        target_url: &str,
        w: Option<u32>,
        h: Option<u32>,
    ) -> Result<RenderCacheEntry> {
        let (width, height) = self.normalize_dimensions(w, h);
        let normalized_target = target_url.trim().to_string();
        let canonical = self.canonical_target_key(screen_type, &normalized_target);
        let cache_key = Self::cache_key(&canonical, width, height);
        let flight_key = cache_key.clone();

        // Single-flight deduplication: prevent duplicate concurrent renders for the same canonical url+size
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

            // A render for this exact canonical url+size is already running, await it
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(cached) = self
                .get_cached_for_screen(
                    screen_id,
                    screen_type,
                    &normalized_target,
                    Some(width),
                    Some(height),
                )
                .await
            {
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
            width = width,
            height = height,
            "Starting dashboard pre-render via browserless-chrome"
        );

        let render_res = {
            let _permit = self.semaphore.acquire().await;
            self.render_screen_sized(screen_type, &normalized_target, width, height)
                .await
        };

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
            target_url: canonical,
            width,
            height,
        };

        {
            let mut lock = self.cache.write().await;
            lock.insert(cache_key, entry.clone());
            if !screen_id.starts_with("preset-") {
                lock.insert(Self::cache_key(screen_id, width, height), entry.clone());
            }
        }

        info!(
            screen_id = %screen_id,
            width = width,
            height = height,
            duration_ms = duration_ms,
            bytes = entry.image_png.len(),
            "Dashboard pre-render completed and cached successfully"
        );
        Ok(entry)
    }

    pub async fn render_screen(&self, screen_type: ScreenType, raw_url: &str) -> Result<Vec<u8>> {
        self.render_screen_sized(
            screen_type,
            raw_url,
            self.config.viewport_width,
            self.config.viewport_height,
        )
        .await
    }

    pub async fn render_screen_sized(
        &self,
        screen_type: ScreenType,
        raw_url: &str,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let script = self.build_puppeteer_script(screen_type, raw_url, width, height)?;
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

    fn build_puppeteer_script(
        &self,
        screen_type: ScreenType,
        raw_url: &str,
        width: u32,
        height: u32,
    ) -> Result<String> {
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
    const u = req.url();
    if (u.includes('_search') || u.includes('/prometheus/api/v1/')) {{
      pendingO2Queries++;
      o2QueriesFired++;
    }}
  }});

  page.on('requestfinished', req => {{
    const u = req.url();
    if (u.includes('_search') || u.includes('/prometheus/api/v1/')) {{
      pendingO2Queries = Math.max(0, pendingO2Queries - 1);
    }}
  }});

  page.on('requestfailed', req => {{
    const u = req.url();
    if (u.includes('_search') || u.includes('/prometheus/api/v1/')) {{
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

  // 1. Wait up to 10s for _search_stream or prometheus queries to start
  for (let i = 0; i < 100; i++) {{
    if (o2QueriesFired > 0) break;
    await new Promise(r => setTimeout(r, 100));
  }}

  // 2. Wait up to 25s for all _search_stream / prometheus queries to finish
  for (let i = 0; i < 125; i++) {{
    if (o2QueriesFired > 0 && pendingO2Queries === 0) break;
    await new Promise(r => setTimeout(r, 200));
  }}

  // 3. Switch to Dark Mode via [data-test="navbar-theme-toggle-btn"]
  try {{
    await page.evaluate(() => {{
      const html = document.documentElement;
      const isDark = html.classList.contains('dark') || document.body.classList.contains('body--dark') || document.body.classList.contains('dark');
      if (!isDark) {{
        const themeBtn = document.querySelector('[data-test="navbar-theme-toggle-btn"]');
        if (themeBtn) themeBtn.click();
      }}
    }});
  }} catch (e) {{}}

  // 4. Hide top bar, left nav, edit buttons, and variable filter bars for a pure kiosk layout
  try {{
    await page.addStyleTag({{
      content: `
        header, nav, [data-test="navbar-main-nav"],
        [data-test="header-openobserve-logo"], [data-test="upgrade-to-enterprise-btn"],
        [data-test="dashboard-back-btn"], [data-test="dashboard-panel-add"],
        [data-test="export-dashboard"], [data-test="dashboard-share-btn"],
        [data-test="dashboard-setting-btn"], [data-test="dashboard-print-btn"],
        [data-test="dashboard-fullscreen-btn"], [data-test="view-dashboard-scheduled-reports"],
        [data-test="dashboard-json-edit-btn"], [data-test="dashboard-view-set-home-btn"],
        [data-test="dashboard-tab-add-btn"], [data-test="global-variables-selector"],
        .q-notifications, [role="alert"] {{
          display: none !important;
        }}
        ::-webkit-scrollbar {{
          display: none !important;
        }}
      `
    }});
  }} catch (e) {{}}

  await new Promise(r => setTimeout(r, 2000));
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

                // Pre-render the tablet's 3 exact stage resolutions:
                // 1. With expanded sidebar (1528x1122)
                // 2. With collapsed sidebar (1710x1122)
                // 3. Without sidebar / full kiosk (1776x1122)
                let resolutions: [(u32, u32); 3] = [(1528, 1122), (1776, 1122), (1710, 1122)];

                for screen in targets {
                    let target_url = if screen.url.trim().is_empty() {
                        &snap.formatted_url
                    } else {
                        &screen.url
                    };
                    for (pw, ph) in resolutions {
                        if let Err(e) = renderer
                            .render_and_cache_sized(
                                &screen.id,
                                screen.screen_type,
                                target_url,
                                Some(pw),
                                Some(ph),
                            )
                            .await
                        {
                            warn!(
                                screen = %screen.name,
                                width = pw,
                                height = ph,
                                error = %e,
                                "Background pre-render failed"
                            );
                        }
                    }
                }

                // Pre-calculate ALL curated Grafana and OpenObserve preset dashboards
                // Pass A: warm primary tablet resolution (1528x1122) first for every preset (refresh if >5m old)
                // Pass B: warm alternate resolutions (1776x1122, 1710x1122) (refresh if >10m old)
                let mut curated_targets: Vec<(String, ScreenType, &str)> =
                    Vec::with_capacity(CURATED_GRAFANA_URLS.len() + CURATED_OPENOBSERVE_IDS.len());
                for (idx, url) in CURATED_GRAFANA_URLS.iter().enumerate() {
                    curated_targets.push((
                        format!("preset-grafana-{idx}"),
                        ScreenType::Grafana,
                        *url,
                    ));
                }
                for (idx, id) in CURATED_OPENOBSERVE_IDS.iter().enumerate() {
                    curated_targets.push((
                        format!("preset-o2-{idx}"),
                        ScreenType::OpenObserve,
                        *id,
                    ));
                }

                for (res_idx, (pw, ph)) in resolutions.iter().copied().enumerate() {
                    let max_age_secs = if res_idx == 0 { 300 } else { 600 };
                    for (preset_id, screen_type, target_url) in &curated_targets {
                        if renderer
                            .is_fresh(*screen_type, target_url, pw, ph, max_age_secs)
                            .await
                        {
                            continue;
                        }
                        match renderer
                            .render_and_cache_sized(
                                preset_id,
                                *screen_type,
                                target_url,
                                Some(pw),
                                Some(ph),
                            )
                            .await
                        {
                            Ok(_) => {
                                debug!(
                                    preset = %preset_id,
                                    width = pw,
                                    height = ph,
                                    "Curated preset pre-render cached"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    preset = %preset_id,
                                    url = %target_url,
                                    width = pw,
                                    height = ph,
                                    error = %e,
                                    "Curated preset pre-render failed"
                                );
                            }
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

    #[tokio::test]
    async fn curated_presets_cache_simultaneously_without_collision() {
        assert_eq!(CURATED_GRAFANA_URLS.len(), 9);
        assert_eq!(CURATED_OPENOBSERVE_IDS.len(), 10);

        let r = DashboardRenderer::new(DashboardRendererConfig::default());
        let key_a = r.canonical_target_key(ScreenType::OpenObserve, CURATED_OPENOBSERVE_IDS[0]);
        let key_b = r.canonical_target_key(
            ScreenType::OpenObserve,
            "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516898066293608448&folder=default",
        );
        assert_eq!(key_a, key_b);

        let key_c = r.canonical_target_key(ScreenType::OpenObserve, CURATED_OPENOBSERVE_IDS[1]);
        assert_ne!(key_a, key_c);

        {
            let mut lock = r.cache.write().await;
            lock.insert(
                DashboardRenderer::cache_key(&key_a, 1528, 1122),
                RenderCacheEntry {
                    image_png: vec![1, 2, 3],
                    rendered_at: Utc::now(),
                    duration_ms: 10,
                    target_url: key_a.clone(),
                    width: 1528,
                    height: 1122,
                },
            );
            lock.insert(
                DashboardRenderer::cache_key(&key_c, 1528, 1122),
                RenderCacheEntry {
                    image_png: vec![4, 5, 6],
                    rendered_at: Utc::now(),
                    duration_ms: 10,
                    target_url: key_c.clone(),
                    width: 1528,
                    height: 1122,
                },
            );
        }

        // Lookup by full URL hits short-ID pre-render, even when requesting alternate resolution
        let hit_a = r
            .get_cached_for_screen(
                "o2-overview",
                ScreenType::OpenObserve,
                "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516898066293608448&folder=default",
                Some(1710),
                Some(1122),
            )
            .await;
        assert_eq!(hit_a.map(|e| e.image_png), Some(vec![1, 2, 3]));

        let hit_c = r
            .get_cached_for_screen(
                "o2-overview",
                ScreenType::OpenObserve,
                CURATED_OPENOBSERVE_IDS[1],
                Some(1528),
                Some(1122),
            )
            .await;
        assert_eq!(hit_c.map(|e| e.image_png), Some(vec![4, 5, 6]));
    }
}
