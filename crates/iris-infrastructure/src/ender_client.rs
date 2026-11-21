use std::env;
use std::time::Duration;

use chrono::Utc;
use iris_core::{IrisError, Printer3DStatus, Result};
use serde::Deserialize;

const DEFAULT_ENDER_URL: &str = "http://ender-power.home.svc.cluster.local:8088";

#[derive(Deserialize, Default)]
struct RawStatusResponse {
    #[serde(default)]
    state: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    plug_on: bool,
    #[serde(default)]
    printer: Option<RawPrinterState>,
    #[serde(default)]
    job: Option<RawJobState>,
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

#[derive(Clone)]
pub struct EnderClient {
    base_url: String,
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

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self { base_url, http }
    }

    pub async fn fetch_status(&self) -> Result<Printer3DStatus> {
        let url = format!("{}/api/status", self.base_url.trim_end_matches('/'));
        let resp = match self.http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(Printer3DStatus {
                    connected: false,
                    state: "OFFLINE".to_string(),
                    message: format!("Orchestrateur 3D inaccessible ({e})"),
                    updated_at_epoch_ms: Utc::now().timestamp_millis(),
                    ..Default::default()
                });
            }
        };

        if !resp.status().is_success() {
            return Ok(Printer3DStatus {
                connected: false,
                state: "ERROR".to_string(),
                message: format!("HTTP {}", resp.status()),
                updated_at_epoch_ms: Utc::now().timestamp_millis(),
                ..Default::default()
            });
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("Erreur lecture statut Ender: {e}")))?;

        parse_ender_status(&body)
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
        let url = format!(
            "{}/api/power/cooldown-and-off",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec cooldown Ender: {e}")))?;
        Ok(resp.status().is_success())
    }

    pub async fn wake_and_print(&self) -> Result<bool> {
        let url = format!(
            "{}/api/print/wake-and-print",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Échec wake-and-print Ender: {e}")))?;
        Ok(resp.status().is_success())
    }
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

    let progress_pct = job
        .completion
        .map(|c| c.clamp(0.0, 100.0).round() as u32)
        .unwrap_or(0);

    Ok(Printer3DStatus {
        connected: true,
        state: raw.state,
        message: raw.message,
        progress_pct,
        time_elapsed_secs: job.print_time.unwrap_or(0),
        time_remaining_secs: job.print_time_left.unwrap_or(0),
        filename: job.file_name.unwrap_or_default(),
        nozzle_temp_c: printer.hotend_actual.round() as i32,
        nozzle_target_c: printer.hotend_target.round() as i32,
        bed_temp_c: printer.bed_actual.round() as i32,
        bed_target_c: printer.bed_target.round() as i32,
        plug_on: raw.plug_on,
        power_w: if raw.plug_on { 180 } else { 0 },
        updated_at_epoch_ms: Utc::now().timestamp_millis(),
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
}
