use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use iris_core::{IrisError, Printer3DFileSummary, Printer3DLogEntry, Printer3DStatus, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

const DEFAULT_ENDER_URL: &str = "http://192.168.0.107:8088";
const DEFAULT_OCTOPRINT_URL: &str = "http://192.168.0.107:5000";
const DEFAULT_OCTOPRINT_USER: &str = "zacharie";
const DEFAULT_OCTOPRINT_PASS: &str = "Superman69230";
const DEFAULT_OCTOPRINT_API_KEY: &str = "_o_EjaComN3u10s21VWy0s5BFex9GxIzJPVVD8IzLQY";
const DEFAULT_WEBCAM_SNAPSHOT_URL: &str = "http://192.168.0.107:5001/snapshot";

#[derive(Deserialize, Default)]
struct RawStatusResponse {
    #[serde(default)]
    state: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    printer_state: Option<String>,
    #[serde(default)]
    auto_shutdown_enabled: Option<bool>,
    #[serde(default)]
    plug_on: Option<bool>,
    #[serde(default)]
    plug_watts: Option<f64>,
    #[serde(default)]
    hotend_actual: Option<f64>,
    #[serde(default)]
    hotend_target: Option<f64>,
    #[serde(default)]
    bed_actual: Option<f64>,
    #[serde(default)]
    bed_target: Option<f64>,
    #[serde(default)]
    job_file: Option<String>,
    #[serde(default)]
    job_completion: Option<f64>,
    #[serde(default)]
    job_state: Option<String>,
    #[serde(default)]
    printer: Option<RawPrinterState>,
    #[serde(default)]
    job: Option<RawJobState>,
    #[serde(default)]
    logs: Vec<RawLogEntry>,
    #[serde(default)]
    recent_logs: Vec<RawLogEntry>,
}

#[derive(Deserialize, Default)]
struct RawLogEntry {
    #[serde(default)]
    time: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    level: String,
}

#[derive(Deserialize, Default)]
struct RawPrinterState {
    #[serde(default)]
    hotend_actual: f64,
    #[serde(default)]
    hotend_target: f64,
    #[serde(default)]
    bed_actual: f64,
    #[serde(default)]
    bed_target: f64,
}

#[derive(Deserialize, Default)]
struct RawJobState {
    #[serde(default)]
    completion: Option<f64>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    print_time: Option<u64>,
    #[serde(default)]
    print_time_left: Option<u64>,
}

#[derive(Clone, Default)]
pub struct GcodeHeaderMeta {
    pub slicer: String,
    pub total_layers: u32,
    pub filament_density: f64,
    pub max_z_height: f64,
    pub thumbnail_data_url: Option<String>,
}

#[derive(Clone)]
pub struct EnderClient {
    base_url: String,
    octoprint_url: String,
    octoprint_user: String,
    octoprint_pass: String,
    webcam_snapshot_url: String,
    api_key: Arc<RwLock<String>>,
    gcode_cache: Arc<RwLock<HashMap<String, GcodeHeaderMeta>>>,
    http: reqwest::Client,
}

impl EnderClient {
    pub fn from_env() -> Self {
        let base_url = env::var("IRIS_ENDER_URL")
            .ok()
            .or_else(|| env::var("ENDER_ORCHESTRATOR_URL").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ENDER_URL.to_string());

        let octoprint_url = env::var("IRIS_OCTOPRINT_URL")
            .ok()
            .or_else(|| env::var("OCTOPRINT_URL").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OCTOPRINT_URL.to_string());

        let octoprint_user = env::var("IRIS_OCTOPRINT_USER")
            .ok()
            .or_else(|| env::var("OCTOPRINT_USER").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OCTOPRINT_USER.to_string());

        let octoprint_pass = env::var("IRIS_OCTOPRINT_PASSWORD")
            .ok()
            .or_else(|| env::var("OCTOPRINT_PASSWORD").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OCTOPRINT_PASS.to_string());

        let initial_key = env::var("IRIS_OCTOPRINT_API_KEY")
            .ok()
            .or_else(|| env::var("OCTOPRINT_API_KEY").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OCTOPRINT_API_KEY.to_string());

        let webcam_snapshot_url = env::var("IRIS_OCTOPRINT_WEBCAM_SNAPSHOT_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_WEBCAM_SNAPSHOT_URL.to_string());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            base_url,
            octoprint_url,
            octoprint_user,
            octoprint_pass,
            webcam_snapshot_url,
            api_key: Arc::new(RwLock::new(initial_key)),
            gcode_cache: Arc::new(RwLock::new(HashMap::new())),
            http,
        }
    }

    async fn resolve_octoprint_api_key(&self) -> Option<String> {
        let current = self.api_key.read().await.clone();
        let ver_url = format!("{}/api/version", self.octoprint_url.trim_end_matches('/'));
        if !current.is_empty() {
            if let Ok(resp) = self
                .http
                .get(&ver_url)
                .header("X-Api-Key", &current)
                .send()
                .await
            {
                if resp.status().is_success() {
                    return Some(current);
                }
            }
        }

        // Fallback: login with username & password via CSRF cookie handshake
        let root_url = format!("{}/", self.octoprint_url.trim_end_matches('/'));
        let root_resp = self.http.get(&root_url).send().await.ok()?;
        let mut cookies = Vec::new();
        let mut csrf_token = String::new();
        for val in root_resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(s) = val.to_str() {
                if let Some(pair) = s.split(';').next() {
                    if let Some((k, v)) = pair.split_once('=') {
                        if k.trim().starts_with("csrf_token") {
                            csrf_token = v.trim().to_string();
                        }
                    }
                    cookies.push(pair.trim().to_string());
                }
            }
        }
        let cookie_header = cookies.join("; ");
        let login_url = format!("{}/api/login", self.octoprint_url.trim_end_matches('/'));
        let login_resp = self
            .http
            .post(&login_url)
            .header("Cookie", cookie_header)
            .header("X-CSRF-Token", csrf_token)
            .json(&serde_json::json!({
                "user": self.octoprint_user,
                "pass": self.octoprint_pass,
                "remember": true
            }))
            .send()
            .await
            .ok()?;

        if !login_resp.status().is_success() {
            return None;
        }
        let json: Value = login_resp.json().await.ok()?;
        let key = json.get("apikey")?.as_str()?.trim().to_string();
        if !key.is_empty() {
            *self.api_key.write().await = key.clone();
            Some(key)
        } else {
            None
        }
    }

    async fn octo_get_json(&self, path: &str, key: &str) -> Option<Value> {
        let url = format!(
            "{}/{}",
            self.octoprint_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let resp = self
            .http
            .get(&url)
            .header("X-Api-Key", key)
            .send()
            .await
            .ok()?;
        if resp.status().is_success() {
            resp.json().await.ok()
        } else {
            None
        }
    }

    async fn octo_post_json(&self, path: &str, payload: Value) -> Result<bool> {
        let Some(key) = self.resolve_octoprint_api_key().await else {
            return Err(IrisError::Persistence(
                "Impossible de s'authentifier auprès d'OctoPrint".to_string(),
            ));
        };
        let url = format!(
            "{}/{}",
            self.octoprint_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .header("X-Api-Key", key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Requête OctoPrint échouée: {e}")))?;
        Ok(resp.status().is_success())
    }

    async fn get_or_fetch_gcode_header(&self, file_path: &str, key: &str) -> GcodeHeaderMeta {
        if file_path.trim().is_empty() {
            return GcodeHeaderMeta::default();
        }
        if let Some(cached) = self.gcode_cache.read().await.get(file_path).cloned() {
            return cached;
        }

        let url = format!(
            "{}/downloads/files/local/{}",
            self.octoprint_url.trim_end_matches('/'),
            file_path.trim_start_matches('/')
        );
        let resp = self
            .http
            .get(&url)
            .header("X-Api-Key", key)
            .header("Range", "bytes=0-180000")
            .send()
            .await;

        let Ok(r) = resp else {
            return GcodeHeaderMeta::default();
        };
        if !r.status().is_success() && r.status().as_u16() != 206 {
            return GcodeHeaderMeta::default();
        }
        let Ok(bytes) = r.bytes().await else {
            return GcodeHeaderMeta::default();
        };
        let slice = if bytes.len() > 180_000 {
            &bytes[..180_000]
        } else {
            &bytes[..]
        };
        let text = String::from_utf8_lossy(slice);
        let meta = parse_gcode_header(&text);
        self.gcode_cache
            .write()
            .await
            .insert(file_path.to_string(), meta.clone());
        meta
    }

    pub async fn fetch_webcam_snapshot(&self) -> Result<(Vec<u8>, String)> {
        let resp = self
            .http
            .get(&self.webcam_snapshot_url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Webcam Ender inaccessible: {e}")))?;
        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "Webcam Ender HTTP {}",
                resp.status()
            )));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| IrisError::Persistence(format!("Lecture flux webcam échouée: {e}")))?;
        Ok((bytes.to_vec(), content_type))
    }

    pub async fn fetch_status(&self) -> Result<Printer3DStatus> {
        // 1. Fetch orchestrator / power daemon status (port 8088)
        let daemon_url = format!("{}/api/status", self.base_url.trim_end_matches('/'));
        let daemon_status = match self.http.get(&daemon_url).send().await {
            Ok(r) if r.status().is_success() => r
                .text()
                .await
                .ok()
                .and_then(|b| parse_ender_status(&b).ok()),
            _ => None,
        };

        // 2. Fetch direct OctoPrint telemetry (port 5000)
        let octo_key = self.resolve_octoprint_api_key().await;
        if let Some(key) = octo_key {
            let (ver_val, conn_val, printer_val, job_val, files_val) = tokio::join!(
                self.octo_get_json("/api/version", &key),
                self.octo_get_json("/api/connection", &key),
                self.octo_get_json("/api/printer", &key),
                self.octo_get_json("/api/job", &key),
                self.octo_get_json("/api/files?recursive=true", &key),
            );

            if conn_val.is_some() || printer_val.is_some() || job_val.is_some() {
                let mut status = daemon_status.unwrap_or_else(|| Printer3DStatus {
                    connected: true,
                    auto_shutdown_enabled: true,
                    octoprint_url: self.octoprint_url.clone(),
                    updated_at_epoch_ms: Utc::now().timestamp_millis(),
                    ..Default::default()
                });
                status.connected = true;
                status.octoprint_url = self.octoprint_url.clone();
                status.updated_at_epoch_ms = Utc::now().timestamp_millis();

                if let Some(ver) = &ver_val {
                    status.octoprint_version = ver
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("OctoPrint")
                        .to_string();
                }

                if let Some(conn) = &conn_val {
                    if let Some(cur) = conn.get("current") {
                        status.printer_port = cur
                            .get("port")
                            .and_then(Value::as_str)
                            .unwrap_or("/tmp/printer")
                            .to_string();
                        if let Some(st) = cur.get("state").and_then(Value::as_str) {
                            if st.eq_ignore_ascii_case("Printing") {
                                status.state = "PRINTING".to_string();
                            } else if st.eq_ignore_ascii_case("Paused")
                                || st.eq_ignore_ascii_case("Pausing")
                            {
                                status.state = "PAUSED".to_string();
                            } else if st.eq_ignore_ascii_case("Operational")
                                && status.state != "COOLING_DOWN"
                            {
                                status.state = "OPERATIONAL".to_string();
                            }
                        }
                    }
                }

                if let Some(pr) = &printer_val {
                    if let Some(temps) = pr.get("temperature") {
                        if let Some(t0) = temps.get("tool0") {
                            if let Some(a) = t0.get("actual").and_then(Value::as_f64) {
                                status.nozzle_temp_c = a.round() as i32;
                            }
                            if let Some(t) = t0.get("target").and_then(Value::as_f64) {
                                status.nozzle_target_c = t.round() as i32;
                            }
                        }
                        if let Some(bed) = temps.get("bed") {
                            if let Some(a) = bed.get("actual").and_then(Value::as_f64) {
                                status.bed_temp_c = a.round() as i32;
                            }
                            if let Some(t) = bed.get("target").and_then(Value::as_f64) {
                                status.bed_target_c = t.round() as i32;
                            }
                        }
                    }
                }

                let mut active_file_path = String::new();
                if let Some(job) = &job_val {
                    if let Some(job_state) = job.get("state").and_then(Value::as_str) {
                        if job_state.eq_ignore_ascii_case("Printing") {
                            status.state = "PRINTING".to_string();
                        } else if job_state.eq_ignore_ascii_case("Paused")
                            || job_state.eq_ignore_ascii_case("Pausing")
                        {
                            status.state = "PAUSED".to_string();
                        } else if job_state.eq_ignore_ascii_case("Operational")
                            && status.state != "COOLING_DOWN"
                        {
                            status.state = "OPERATIONAL".to_string();
                        }
                    }

                    if let Some(j) = job.get("job") {
                        status.user = j
                            .get("user")
                            .and_then(Value::as_str)
                            .unwrap_or(&self.octoprint_user)
                            .to_string();
                        status.estimated_total_secs = j
                            .get("estimatedPrintTime")
                            .and_then(Value::as_f64)
                            .map(|v| v.round() as u64)
                            .unwrap_or(0);

                        if let Some(f) = j.get("file") {
                            status.filename = f
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            status.display_name = f
                                .get("display")
                                .and_then(Value::as_str)
                                .unwrap_or(&status.filename)
                                .to_string();
                            active_file_path = f
                                .get("path")
                                .and_then(Value::as_str)
                                .unwrap_or(&status.filename)
                                .to_string();
                            status.file_size_bytes =
                                f.get("size").and_then(Value::as_u64).unwrap_or(0);
                        }

                        if let Some(fil) = j.get("filament").and_then(|fl| fl.get("tool0")) {
                            let len_mm = fil.get("length").and_then(Value::as_f64).unwrap_or(0.0);
                            let vol_cm3 = fil.get("volume").and_then(Value::as_f64).unwrap_or(0.0);
                            if len_mm > 0.0 {
                                status.filament_length_m = format!("{:.2}", len_mm / 1000.0);
                            }
                            if vol_cm3 > 0.0 {
                                status.filament_volume_cm3 = format!("{:.1}", vol_cm3);
                                // PLA default density 1.24 g/cm3
                                status.filament_weight_g = format!("{:.1}", vol_cm3 * 1.24);
                            }
                        }
                    }

                    if let Some(prog) = job.get("progress") {
                        let comp = prog
                            .get("completion")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0)
                            .clamp(0.0, 100.0);
                        status.progress_pct = comp.round() as u32;
                        status.progress_exact = format!("{comp:.1}");
                        status.file_pos_bytes =
                            prog.get("filepos").and_then(Value::as_u64).unwrap_or(0);
                        status.time_elapsed_secs =
                            prog.get("printTime").and_then(Value::as_u64).unwrap_or(0);
                        status.time_remaining_secs = prog
                            .get("printTimeLeft")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                    }
                }

                // Parse files list & active file gcodeAnalysis
                if let Some(files_root) = &files_val {
                    if let Some(arr) = files_root.get("files").and_then(Value::as_array) {
                        let mut summaries = Vec::new();
                        for f in arr {
                            let name = f
                                .get("display")
                                .or_else(|| f.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let path = f
                                .get("path")
                                .or_else(|| f.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            if name.is_empty() {
                                continue;
                            }
                            let size_bytes = f.get("size").and_then(Value::as_u64).unwrap_or(0);
                            let uploaded_at_epoch_secs =
                                f.get("date").and_then(Value::as_i64).unwrap_or(0);
                            let last_print_success = f
                                .get("prints")
                                .and_then(|p| p.get("last"))
                                .and_then(|l| l.get("success"))
                                .and_then(Value::as_bool);

                            let mut estimated_time_secs = 0;
                            let mut filament_length_mm = 0;
                            let mut dimensions_mm = String::new();

                            if let Some(ga) = f.get("gcodeAnalysis") {
                                estimated_time_secs = ga
                                    .get("estimatedPrintTime")
                                    .and_then(Value::as_f64)
                                    .map(|v| v.round() as u64)
                                    .unwrap_or(0);
                                filament_length_mm = ga
                                    .get("filament")
                                    .and_then(|fl| fl.get("tool0"))
                                    .and_then(|t0| t0.get("length"))
                                    .and_then(Value::as_f64)
                                    .map(|v| v.round() as u64)
                                    .unwrap_or(0);
                                if let Some(dim) = ga.get("dimensions") {
                                    let w = dim.get("width").and_then(Value::as_f64).unwrap_or(0.0);
                                    let d = dim.get("depth").and_then(Value::as_f64).unwrap_or(0.0);
                                    let h =
                                        dim.get("height").and_then(Value::as_f64).unwrap_or(0.0);
                                    if w > 0.0 || d > 0.0 || h > 0.0 {
                                        dimensions_mm = format!("{w:.1} × {d:.1} × {h:.1} mm");
                                    }
                                    if path == active_file_path || name == status.filename {
                                        status.model_width_mm = format!("{w:.1}");
                                        status.model_depth_mm = format!("{d:.1}");
                                        status.model_height_mm = format!("{h:.1}");
                                    }
                                }
                            }

                            summaries.push(Printer3DFileSummary {
                                name,
                                path,
                                size_bytes,
                                estimated_time_secs,
                                filament_length_mm,
                                dimensions_mm,
                                uploaded_at_epoch_secs,
                                last_print_success,
                            });
                        }
                        summaries.sort_by(|a, b| {
                            b.uploaded_at_epoch_secs.cmp(&a.uploaded_at_epoch_secs)
                        });
                        summaries.truncate(8);
                        status.recent_files = summaries;
                    }
                }

                // Extract G-code embedded thumbnail & slicer header for active file
                let target_file = if !active_file_path.is_empty() {
                    active_file_path
                } else {
                    status
                        .recent_files
                        .first()
                        .map(|f| f.path.clone())
                        .unwrap_or_default()
                };
                if !target_file.is_empty() {
                    let meta = self.get_or_fetch_gcode_header(&target_file, &key).await;
                    if !meta.slicer.is_empty() {
                        status.slicer = meta.slicer;
                    }
                    if meta.total_layers > 0 {
                        status.total_layers = meta.total_layers;
                        let comp_ratio = status.progress_exact_f64().clamp(0.0, 100.0) / 100.0;
                        if status.state == "PRINTING" || status.state == "PAUSED" {
                            status.current_layer = ((meta.total_layers as f64 * comp_ratio).ceil()
                                as u32)
                                .clamp(1, meta.total_layers);
                        }
                    }
                    if meta.max_z_height > 0.0 {
                        if status.model_height_mm.is_empty() {
                            status.model_height_mm = format!("{:.2}", meta.max_z_height);
                        }
                        let comp_ratio = status.progress_exact_f64().clamp(0.0, 100.0) / 100.0;
                        status.current_z_mm =
                            format!("{:.2}", (meta.max_z_height * comp_ratio).max(0.2));
                    }
                    if meta.thumbnail_data_url.is_some() {
                        status.thumbnail_data_url = meta.thumbnail_data_url;
                    }
                }

                // If printer is actively heating or printing, ensure plug_on reflects active power
                if status.state == "PRINTING" || status.nozzle_temp_c > 45 || status.bed_temp_c > 40
                {
                    status.plug_on = true;
                    if status.power_w <= 0 {
                        status.power_w = if status.nozzle_target_c > 0 || status.bed_target_c > 0 {
                            185
                        } else {
                            25
                        };
                    }
                }

                // Friendly message
                if status.state == "PRINTING" {
                    let clean_name = clean_piece_title(&status.display_name);
                    status.message = format!(
                        "Impression en cours : {} ({}%)",
                        clean_name, status.progress_exact
                    );
                } else if status.state == "PAUSED" {
                    status.message = format!(
                        "Impression en pause : {} ({}%)",
                        clean_piece_title(&status.display_name),
                        status.progress_exact
                    );
                } else if status.state == "OPERATIONAL" && status.message.is_empty() {
                    status.message = "Ender 3 opérationnelle et connectée à OctoPrint".to_string();
                }

                status.webcam_available = true;
                return Ok(status);
            }
        }

        if let Some(status) = daemon_status {
            return Ok(status);
        }

        Ok(Printer3DStatus {
            connected: false,
            state: "OFFLINE".to_string(),
            message: "OctoPrint (192.168.0.107:5000) et Orchestrateur Ender inaccessibles"
                .to_string(),
            octoprint_url: self.octoprint_url.clone(),
            updated_at_epoch_ms: Utc::now().timestamp_millis(),
            ..Default::default()
        })
    }

    pub async fn power_on(&self) -> Result<bool> {
        let url = format!("{}/api/power/on", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec power on Ender: {e}")))?;
        Ok(resp.status().is_success())
    }

    pub async fn power_off(&self, force: bool) -> Result<bool> {
        let url = format!(
            "{}/api/power/off?force={}",
            self.base_url.trim_end_matches('/'),
            force
        );
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec power off Ender: {e}")))?;
        Ok(resp.status().is_success())
    }

    pub async fn cooldown_and_off(&self) -> Result<bool> {
        // Support both /api/cooldown (ender_power_daemon) and /api/power/cooldown-and-off
        let url1 = format!("{}/api/cooldown", self.base_url.trim_end_matches('/'));
        if let Ok(r) = self.http.post(&url1).send().await {
            if r.status().is_success() {
                return Ok(true);
            }
        }
        let url2 = format!(
            "{}/api/power/cooldown-and-off",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url2)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec cooldown Ender: {e}")))?;
        Ok(resp.status().is_success())
    }

    pub async fn wake_and_print(&self) -> Result<bool> {
        let url1 = format!("{}/api/wake", self.base_url.trim_end_matches('/'));
        if let Ok(r) = self.http.post(&url1).send().await {
            if r.status().is_success() {
                return Ok(true);
            }
        }
        let url2 = format!(
            "{}/api/print/wake-and-print",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url2)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec wake-and-print Ender: {e}")))?;
        Ok(resp.status().is_success())
    }

    pub async fn job_command(&self, action: &str) -> Result<bool> {
        let payload = match action.trim().to_lowercase().as_str() {
            "start" => serde_json::json!({ "command": "start" }),
            "cancel" => serde_json::json!({ "command": "cancel" }),
            "pause" => serde_json::json!({ "command": "pause", "action": "pause" }),
            "resume" => serde_json::json!({ "command": "pause", "action": "resume" }),
            "toggle" => serde_json::json!({ "command": "pause", "action": "toggle" }),
            other => {
                return Err(IrisError::InvalidConfig(format!(
                    "Commande job OctoPrint inconnue: {other}"
                )))
            }
        };
        self.octo_post_json("/api/job", payload).await
    }

    pub async fn select_file(&self, file_path: &str, print_now: bool) -> Result<bool> {
        let path = format!(
            "/api/files/local/{}",
            file_path.trim().trim_start_matches('/')
        );
        self.octo_post_json(
            &path,
            serde_json::json!({
                "command": "select",
                "print": print_now
            }),
        )
        .await
    }
}

trait ProgressExactExt {
    fn progress_exact_f64(&self) -> f64;
}

impl ProgressExactExt for Printer3DStatus {
    fn progress_exact_f64(&self) -> f64 {
        self.progress_exact
            .parse::<f64>()
            .unwrap_or(self.progress_pct as f64)
    }
}

fn clean_piece_title(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .trim_end_matches(".gcode")
        .trim_end_matches(".GCODE");
    if trimmed.is_empty() {
        "Pièce 3D".to_string()
    } else {
        trimmed.replace('_', " ")
    }
}

pub fn parse_gcode_header(chunk: &str) -> GcodeHeaderMeta {
    let mut meta = GcodeHeaderMeta::default();
    let mut in_thumb = false;
    let mut current_thumb_b64 = String::new();
    let mut best_thumb_b64 = String::new();

    for line in chunk.lines() {
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix(';') {
            let c = comment.trim();
            if c.starts_with("generated by ") {
                meta.slicer = c
                    .trim_start_matches("generated by ")
                    .split(" on ")
                    .next()
                    .unwrap_or(c)
                    .trim()
                    .to_string();
            } else if let Some(v) = c.strip_prefix("total layer number:") {
                meta.total_layers = v.trim().parse::<u32>().unwrap_or(0);
            } else if let Some(v) = c.strip_prefix("filament_density:") {
                meta.filament_density = v.trim().parse::<f64>().unwrap_or(1.24);
            } else if let Some(v) = c.strip_prefix("max_z_height:") {
                meta.max_z_height = v.trim().parse::<f64>().unwrap_or(0.0);
            } else if c.starts_with("thumbnail begin ") {
                in_thumb = true;
                current_thumb_b64.clear();
            } else if c.starts_with("thumbnail end") {
                in_thumb = false;
                if current_thumb_b64.len() > best_thumb_b64.len() {
                    best_thumb_b64 = current_thumb_b64.clone();
                }
                current_thumb_b64.clear();
            } else if in_thumb
                && !c.is_empty()
                && !c.starts_with("THUMBNAIL_BLOCK")
                && !c.contains(' ')
            {
                current_thumb_b64.push_str(c);
            }
        }
    }

    if !best_thumb_b64.is_empty() {
        meta.thumbnail_data_url = Some(format!("data:image/png;base64,{best_thumb_b64}"));
    }
    meta
}

impl Default for EnderClient {
    fn default() -> Self {
        Self::from_env()
    }
}

pub fn parse_ender_status(body: &str) -> Result<Printer3DStatus> {
    let raw: RawStatusResponse = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("JSON Ender invalide: {e}")))?;

    let printer = raw.printer.unwrap_or_default();
    let job = raw.job.unwrap_or_default();

    let hotend_actual = raw.hotend_actual.unwrap_or(printer.hotend_actual);
    let hotend_target = raw.hotend_target.unwrap_or(printer.hotend_target);
    let bed_actual = raw.bed_actual.unwrap_or(printer.bed_actual);
    let bed_target = raw.bed_target.unwrap_or(printer.bed_target);

    let completion = raw.job_completion.or(job.completion).unwrap_or(0.0);
    let progress_pct = completion.clamp(0.0, 100.0).round() as u32;
    let progress_exact = format!("{:.1}", completion.clamp(0.0, 100.0));

    let filename = raw.job_file.or(job.file_name).unwrap_or_default();

    let plug_on = raw.plug_on.unwrap_or(false)
        || hotend_actual > 45.0
        || bed_actual > 40.0
        || raw.printer_state.as_deref() == Some("Printing");
    let power_w = raw
        .plug_watts
        .map(|w| w.round() as i32)
        .filter(|&w| w > 0)
        .unwrap_or(if plug_on { 180 } else { 0 });

    let mut state = raw.state.clone();
    if raw.printer_state.as_deref() == Some("Printing")
        || raw.job_state.as_deref() == Some("Printing")
    {
        state = "PRINTING".to_string();
    } else if raw.printer_state.as_deref() == Some("Operational") && state == "POWER_OFF" {
        state = "OPERATIONAL".to_string();
    }

    let raw_logs = if !raw.logs.is_empty() {
        raw.logs
    } else {
        raw.recent_logs
    };
    let logs = raw_logs
        .into_iter()
        .map(|l| Printer3DLogEntry {
            time: l.time,
            text: l.text,
            level: l.level,
        })
        .collect();

    Ok(Printer3DStatus {
        connected: true,
        state,
        message: raw.message,
        progress_pct,
        progress_exact,
        time_elapsed_secs: job.print_time.unwrap_or(0),
        time_remaining_secs: job.print_time_left.unwrap_or(0),
        filename: filename.clone(),
        display_name: filename,
        nozzle_temp_c: hotend_actual.round() as i32,
        nozzle_target_c: hotend_target.round() as i32,
        bed_temp_c: bed_actual.round() as i32,
        bed_target_c: bed_target.round() as i32,
        plug_on,
        power_w,
        auto_shutdown_enabled: raw.auto_shutdown_enabled.unwrap_or(true),
        logs,
        updated_at_epoch_ms: Utc::now().timestamp_millis(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ender_status_payload() {
        let json = r#"{
            "state": "PRINTING",
            "message": "Impression de vase.gcode (42.5%)",
            "plug_on": true,
            "printer": {
                "hotend_actual": 209.8,
                "hotend_target": 210.0,
                "bed_actual": 60.1,
                "bed_target": 60.0
            },
            "job": {
                "completion": 42.5,
                "file_name": "vase.gcode",
                "print_time": 1200,
                "print_time_left": 1800
            }
        }"#;

        let status = parse_ender_status(json).unwrap();
        assert!(status.connected);
        assert_eq!(status.state, "PRINTING");
        assert_eq!(status.progress_pct, 43);
        assert_eq!(status.nozzle_temp_c, 210);
        assert_eq!(status.bed_temp_c, 60);
        assert_eq!(status.filename, "vase.gcode");
        assert!(status.plug_on);
    }

    #[test]
    fn parses_flat_daemon_payload_and_gcode_header() {
        let json = r#"{
            "state": "POWER_OFF",
            "message": "Imprimante éteinte",
            "printer_state": "Printing",
            "plug_on": false,
            "hotend_actual": 210.1,
            "hotend_target": 210.0,
            "bed_actual": 60.0,
            "bed_target": 60.0,
            "job_file": "vinyl_33_tours.gcode",
            "job_completion": 18.4,
            "job_state": "Printing"
        }"#;

        let status = parse_ender_status(json).unwrap();
        assert_eq!(status.state, "PRINTING");
        assert_eq!(status.filename, "vinyl_33_tours.gcode");
        assert_eq!(status.progress_pct, 18);
        assert_eq!(status.progress_exact, "18.4");
        assert!(status.plug_on);

        let gcode = "; generated by OrcaSlicer 2.5.2 on 2026-11-26\n; total layer number: 18\n; max_z_height: 2.92\n; thumbnail begin 32x32 12\n; iVBORw0KGgo=\n; thumbnail end\n";
        let meta = parse_gcode_header(gcode);
        assert_eq!(meta.slicer, "OrcaSlicer 2.5.2");
        assert_eq!(meta.total_layers, 18);
        assert_eq!(
            meta.thumbnail_data_url.as_deref(),
            Some("data:image/png;base64,iVBORw0KGgo=")
        );
    }
}
