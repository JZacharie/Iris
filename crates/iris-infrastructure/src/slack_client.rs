use std::env;
use std::time::Duration;

use chrono::{DateTime, Utc};
use iris_core::{IrisError, Result, SlackMessageItem};
use serde::Deserialize;

const DEFAULT_SLACK_CHANNEL_ID: &str = "C0AC8NGNESY";

#[derive(Deserialize)]
struct SlackHistoryResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    messages: Vec<RawSlackMessage>,
}

#[derive(Deserialize)]
struct RawSlackMessage {
    #[serde(default)]
    ts: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    bot_profile: Option<RawBotProfile>,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Deserialize)]
struct RawBotProfile {
    #[serde(default)]
    name: String,
}

pub struct SlackClient {
    token: Option<String>,
    channel_id: String,
    channel_label: String,
    http: reqwest::Client,
}

impl SlackClient {
    pub fn from_env() -> Self {
        let token = env::var("IRIS_SLACK_BOT_TOKEN")
            .ok()
            .or_else(|| env::var("SLACK_BOT_TOKEN").ok())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && !t.contains("REDACTED"));

        let channel_id = env::var("IRIS_SLACK_CHANNEL_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SLACK_CHANNEL_ID.to_string());

        let channel_label = env::var("IRIS_SLACK_CHANNEL_NAME")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "#ai".to_string());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_default();

        Self {
            token,
            channel_id,
            channel_label,
            http,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.token.is_some()
    }

    pub async fn fetch_recent_messages(&self) -> Result<Vec<SlackMessageItem>> {
        let Some(token) = &self.token else {
            return Ok(Vec::new());
        };

        let url = format!(
            "https://slack.com/api/conversations.history?channel={}&limit=8",
            self.channel_id
        );

        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("Slack request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "Slack HTTP {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("Slack body error: {e}")))?;

        parse_slack_history(&body, &self.channel_label)
    }
}

impl Default for SlackClient {
    fn default() -> Self {
        Self::from_env()
    }
}

pub fn parse_slack_history(body: &str, channel_label: &str) -> Result<Vec<SlackMessageItem>> {
    let parsed: SlackHistoryResponse = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("Invalid Slack JSON: {e}")))?;

    if !parsed.ok {
        return Err(IrisError::Persistence(format!(
            "Slack API error: {}",
            parsed.error.unwrap_or_else(|| "unknown".to_string())
        )));
    }

    let items: Vec<SlackMessageItem> = parsed
        .messages
        .into_iter()
        .filter(|m| !m.text.trim().is_empty())
        .take(6)
        .map(|m| {
            let sender = m
                .bot_profile
                .map(|b| b.name)
                .filter(|s| !s.is_empty())
                .or(m.username)
                .or(m.user)
                .unwrap_or_else(|| "Slack".to_string());

            let ts_label = format_slack_ts(&m.ts);
            let clean_text = m
                .text
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">");

            SlackMessageItem {
                id: m.ts,
                channel: channel_label.to_string(),
                sender,
                text: clean_text,
                ts_label,
            }
        })
        .collect();

    Ok(items)
}

fn format_slack_ts(ts: &str) -> String {
    let secs_str = ts.split('.').next().unwrap_or("0");
    let Ok(secs) = secs_str.parse::<i64>() else {
        return String::new();
    };
    let Some(dt) = DateTime::<Utc>::from_timestamp(secs, 0) else {
        return String::new();
    };
    dt.format("%d/%m %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slack_conversations_history() {
        let sample = r#"{
            "ok": true,
            "messages": [
                {
                    "type": "message",
                    "text": ":warning: *KubePodCrashLooping* on jo3 &amp; alerts",
                    "ts": "1773784697.690199",
                    "bot_profile": { "name": "Kusanagi" }
                },
                {
                    "type": "message",
                    "text": "Cluster sync completed",
                    "ts": "1773781000.000100",
                    "user": "U03F1B3R9"
                }
            ]
        }"#;

        let res = parse_slack_history(sample, "#ai");
        assert!(res.is_ok());
        let msgs = res.unwrap_or_default();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].sender, "Kusanagi");
        assert_eq!(msgs[0].channel, "#ai");
        assert!(msgs[0].text.contains("jo3 & alerts"));
    }
}
