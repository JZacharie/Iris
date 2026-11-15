use std::env;
use std::time::Duration;

use chrono::{DateTime, Utc};
use iris_core::{AgentUpgradeReport, IrisError, Result};
use serde::Deserialize;

const DEFAULT_AMARAKI_URL: &str = "http://amaraki.amaraki.svc.cluster.local";

#[derive(Deserialize)]
struct RawReportsEnvelope {
    #[serde(default)]
    reports: Vec<RawAmarakiExecution>,
    #[serde(default)]
    recent_executions: Vec<RawAmarakiExecution>,
}

#[derive(Deserialize)]
struct RawAmarakiExecution {
    id: String,
    agent_name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    prompt_preview: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    started_at: String,
    #[serde(default)]
    completed_at: Option<String>,
    #[serde(default)]
    duration_secs: Option<f64>,
    #[serde(default)]
    is_real_success: Option<bool>,
    #[serde(default)]
    summary_note: Option<String>,
    #[serde(default)]
    report_output: Option<String>,
}

#[derive(Deserialize)]
struct TriggerResponse {
    #[serde(default)]
    job_id: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

pub struct AmarakiClient {
    base_url: String,
    user: String,
    password: String,
    http: reqwest::Client,
}

impl AmarakiClient {
    pub fn from_env() -> Self {
        let base_url = env::var("IRIS_AMARAKI_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_AMARAKI_URL.to_string());
        let user = env::var("IRIS_AMARAKI_USER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "admin".to_string());
        let password = env::var("IRIS_AMARAKI_PASSWORD")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "section9".to_string());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            user,
            password,
            http,
        }
    }

    async fn try_fetch_public_reports(&self) -> Option<Vec<AgentUpgradeReport>> {
        let reports_url = format!("{}/api/reports", self.base_url);
        let resp = self.http.get(&reports_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body = resp.text().await.ok()?;
        parse_amaraki_reports_json(&body).ok()
    }

    pub async fn fetch_reports(&self) -> Result<Vec<AgentUpgradeReport>> {
        if let Some(reports) = self.try_fetch_public_reports().await {
            return Ok(reports);
        }

        let stats_url = format!("{}/api/dashboard/stats", self.base_url);
        let resp = self
            .http
            .get(&stats_url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Amaraki GET {stats_url} failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "Amaraki GET {stats_url} returned {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("Amaraki body error: {e}")))?;

        parse_amaraki_reports_json(&body)
    }

    pub async fn trigger_agent(
        &self,
        agent_name: &str,
        prompt: &str,
        channel: Option<&str>,
    ) -> Result<String> {
        let url = format!("{}/api/agents/test-trigger", self.base_url);
        let payload = serde_json::json!({
            "agent_name": agent_name,
            "prompt": prompt,
            "channel": channel.unwrap_or("#ai"),
        });

        let resp = self
            .http
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .json(&payload)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Amaraki trigger failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("Amaraki trigger body error: {e}")))?;

        if !status.is_success() {
            return Err(IrisError::Persistence(format!(
                "Amaraki trigger HTTP {status}: {body}"
            )));
        }

        let parsed: TriggerResponse = serde_json::from_str(&body)
            .map_err(|e| IrisError::Persistence(format!("Invalid Amaraki trigger JSON: {e}")))?;
        if let Some(err) = parsed.error {
            return Err(IrisError::Persistence(err));
        }

        Ok(parsed.job_id.unwrap_or_else(|| "spawned".to_string()))
    }
}

impl Default for AmarakiClient {
    fn default() -> Self {
        Self::from_env()
    }
}

pub fn parse_amaraki_reports_json(body: &str) -> Result<Vec<AgentUpgradeReport>> {
    let env: RawReportsEnvelope = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("Invalid Amaraki reports JSON: {e}")))?;

    let raw_items = if !env.reports.is_empty() {
        env.reports
    } else {
        env.recent_executions
    };

    let reports = raw_items
        .into_iter()
        .map(|item| {
            let timestamp_str = item.completed_at.as_deref().unwrap_or(&item.started_at);
            let updated_at_epoch_ms = DateTime::parse_from_rfc3339(timestamp_str)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|_| Utc::now().timestamp_millis());

            let status = if item.status.trim().is_empty() {
                "running".to_string()
            } else {
                item.status.to_ascii_lowercase()
            };
            let is_real_success = item.is_real_success.unwrap_or_else(|| status != "failed");
            let prompt = if item.prompt.trim().is_empty() {
                item.prompt_preview
            } else {
                item.prompt
            };
            let duration_secs = item.duration_secs.map(|d| d.round().max(0.0) as u64);

            AgentUpgradeReport {
                id: item.id,
                agent_name: item.agent_name,
                model: item.model,
                channel: item.channel,
                prompt,
                status,
                is_real_success,
                summary_note: item.summary_note.unwrap_or_default(),
                report_output: item.report_output.unwrap_or_default(),
                started_at: item.started_at,
                completed_at: item.completed_at,
                duration_secs,
                updated_at_epoch_ms,
            }
        })
        .collect();

    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amaraki_reports_envelope() {
        let json = r#"{
            "status": "ok",
            "reports": [
                {
                    "id": "antigravity-pr-updater-1234",
                    "agent_name": "antigravity-pr-updater",
                    "model": "gemini-3.1-pro",
                    "channel": "C0AC8NGNESY",
                    "prompt_preview": "Mets à jour les PR...",
                    "prompt": "Mets à jour les PR des projets JZacharie",
                    "status": "succeeded",
                    "started_at": "2026-11-15T10:00:00Z",
                    "completed_at": "2026-11-15T10:02:30Z",
                    "duration_secs": 150.4,
                    "is_real_success": true,
                    "summary_note": "Mission accomplie avec succès",
                    "report_output": "PR #42 rebasée sur jo3"
                }
            ]
        }"#;

        let reports = parse_amaraki_reports_json(json).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].agent_name, "antigravity-pr-updater");
        assert_eq!(reports[0].duration_secs, Some(150));
        assert!(reports[0].is_real_success);
        assert_eq!(reports[0].report_output, "PR #42 rebasée sur jo3");
    }

    #[test]
    fn parses_amaraki_dashboard_stats_fallback() {
        let json = r##"{
            "recent_executions": [
                {
                    "id": "antigravity-dev-99",
                    "agent_name": "antigravity-dev",
                    "model": "gemini-3.1-pro",
                    "channel": "#ai",
                    "prompt_preview": "Dev feature X",
                    "status": "failed",
                    "started_at": "2026-11-15T11:00:00Z"
                }
            ]
        }"##;

        let reports = parse_amaraki_reports_json(json).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].id, "antigravity-dev-99");
        assert!(!reports[0].is_real_success);
        assert_eq!(reports[0].prompt, "Dev feature X");
    }
}
