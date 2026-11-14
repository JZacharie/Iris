use chrono::Utc;
use serde_json::Value;

use crate::model::{ActiveAlert, AlertSeverity, ScreenType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrisCommand {
    SwitchScreen {
        target: String,
    },
    NextScreen,
    PreviousScreen,
    OpenUrl {
        url: String,
        title: String,
        provider: ScreenType,
    },
    TriggerAlert(ActiveAlert),
    ClearAlert,
    SetRotation {
        enabled: bool,
        interval_seconds: Option<u32>,
    },
    SetBrightness {
        percent: i32,
    },
    Reload,
    UpsertNote {
        id: Option<String>,
        text: String,
        category: String,
        done: bool,
        pinned: bool,
    },
    DeleteNote {
        id: String,
    },
}

/// Parses incoming MQTT topic/payload pairs into strongly-typed [`IrisCommand`] values.
/// Supports both JSON objects and plain-text payloads.
pub struct MqttCommandParser;

impl MqttCommandParser {
    pub fn parse(topic: &str, payload: &str) -> Option<IrisCommand> {
        let trimmed = payload.trim();
        let suffix = topic.rsplit('/').next().unwrap_or("").to_lowercase();

        match suffix.as_str() {
            "screen" => parse_screen_command(trimmed),
            "url" => parse_url_command(trimmed),
            "alert" => parse_alert_command(trimmed),
            "rotation" => parse_rotation_command(trimmed),
            "brightness" => parse_brightness_command(trimmed),
            "reload" => Some(IrisCommand::Reload),
            "note" => parse_note_command(trimmed),
            _ => None,
        }
    }

    pub fn infer_provider(url: &str) -> ScreenType {
        let lower = url.to_lowercase();
        if lower.contains("openobserve") || lower.contains("o2-") {
            ScreenType::OpenObserve
        } else if lower.contains("grafana") {
            ScreenType::Grafana
        } else {
            ScreenType::Web
        }
    }
}

fn parse_screen_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let target = extract_json_str(payload, &["screen", "id", "target", "name"])
        .unwrap_or_else(|| payload.to_string());
    match target.to_lowercase().as_str() {
        "next" => Some(IrisCommand::NextScreen),
        "prev" | "previous" => Some(IrisCommand::PreviousScreen),
        _ => Some(IrisCommand::SwitchScreen { target }),
    }
}

fn parse_url_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let Some(obj) = parse_json_object(payload) else {
        return Some(IrisCommand::OpenUrl {
            url: payload.to_string(),
            title: "Écran MQTT".to_string(),
            provider: MqttCommandParser::infer_provider(payload),
        });
    };

    let url = obj
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if url.is_empty() {
        return None;
    }
    let title = obj
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Écran MQTT")
        .to_string();

    let provider = match obj
        .get("provider")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        Some("grafana") => ScreenType::Grafana,
        Some("openobserve" | "o2") => ScreenType::OpenObserve,
        Some("web") => ScreenType::Web,
        _ => MqttCommandParser::infer_provider(url),
    };

    Some(IrisCommand::OpenUrl {
        url: url.to_string(),
        title,
        provider,
    })
}

fn parse_alert_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let lower = payload.to_lowercase();
    if matches!(
        lower.as_str(),
        "clear" | "off" | "false" | "resolved" | "ok" | "0"
    ) {
        return Some(IrisCommand::ClearAlert);
    }

    let now = Utc::now().timestamp_millis();
    let Some(obj) = parse_json_object(payload) else {
        return Some(IrisCommand::TriggerAlert(ActiveAlert {
            id: format!("mqtt-{now}"),
            severity: AlertSeverity::Critical,
            title: payload.to_string(),
            message: String::new(),
            source: "mqtt".to_string(),
            dashboard_url: None,
            screen_id: None,
            triggered_at_epoch_ms: now,
            acknowledged: false,
        }));
    };

    let active = obj.get("active").and_then(Value::as_bool);
    let status = obj
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    if active == Some(false) || matches!(status.as_deref(), Some("resolved" | "ok" | "cleared")) {
        return Some(IrisCommand::ClearAlert);
    }

    let severity = match obj
        .get("severity")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("warning" | "warn") => AlertSeverity::Warning,
        Some("info") => AlertSeverity::Info,
        _ => AlertSeverity::Critical,
    };

    let title = obj
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| obj.get("alertname").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Incident détecté")
        .to_string();

    let message = obj
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| obj.get("description").and_then(Value::as_str))
        .map(str::trim)
        .unwrap_or("")
        .to_string();

    let source = obj
        .get("source")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("mqtt")
        .to_string();

    let dashboard_url = obj
        .get("dashboardUrl")
        .or_else(|| obj.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let screen_id = obj
        .get("screenId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("alert-{now}"));

    Some(IrisCommand::TriggerAlert(ActiveAlert {
        id,
        severity,
        title,
        message,
        source,
        dashboard_url,
        screen_id,
        triggered_at_epoch_ms: now,
        acknowledged: false,
    }))
}

fn parse_rotation_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let Some(obj) = parse_json_object(payload) else {
        return match payload.to_lowercase().as_str() {
            "on" | "true" | "start" | "resume" | "1" => Some(IrisCommand::SetRotation {
                enabled: true,
                interval_seconds: None,
            }),
            "off" | "false" | "stop" | "pause" | "0" => Some(IrisCommand::SetRotation {
                enabled: false,
                interval_seconds: None,
            }),
            _ => payload
                .parse::<u32>()
                .ok()
                .map(|sec| IrisCommand::SetRotation {
                    enabled: sec > 0,
                    interval_seconds: Some(sec.max(5)),
                }),
        };
    };

    let enabled = obj.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let interval_seconds = obj
        .get("intervalSeconds")
        .or_else(|| obj.get("interval"))
        .and_then(Value::as_u64)
        .map(|v| (v as u32).clamp(5, 3600));

    Some(IrisCommand::SetRotation {
        enabled,
        interval_seconds,
    })
}

fn parse_brightness_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let raw = if let Some(obj) = parse_json_object(payload) {
        obj.get("percent")
            .or_else(|| obj.get("brightness"))
            .and_then(Value::as_i64)
            .map(|v| v as i32)?
    } else {
        match payload.to_lowercase().as_str() {
            "dim" | "night" => 15,
            "medium" => 55,
            "full" | "max" | "day" => 100,
            "auto" | "system" => -1,
            _ => payload.parse::<i32>().ok()?,
        }
    };
    let clamped = if raw < 0 { -1 } else { raw.clamp(5, 100) };
    Some(IrisCommand::SetBrightness { percent: clamped })
}

fn parse_note_command(payload: &str) -> Option<IrisCommand> {
    if payload.is_empty() {
        return None;
    }
    let Some(obj) = parse_json_object(payload) else {
        return Some(IrisCommand::UpsertNote {
            id: None,
            text: payload.to_string(),
            category: "MQTT".to_string(),
            done: false,
            pinned: false,
        });
    };

    let action = obj
        .get("action")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if action.as_deref() == Some("delete") {
        if let Some(note_id) = id {
            return Some(IrisCommand::DeleteNote { id: note_id });
        }
    }

    let text = obj
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let category = obj
        .get("category")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("MQTT")
        .to_string();
    let done = obj.get("done").and_then(Value::as_bool).unwrap_or(false);
    let pinned = obj.get("pinned").and_then(Value::as_bool).unwrap_or(false);

    Some(IrisCommand::UpsertNote {
        id,
        text: text.to_string(),
        category,
        done,
        pinned,
    })
}

fn parse_json_object(raw: &str) -> Option<serde_json::Map<String, Value>> {
    if !raw.starts_with('{') {
        return None;
    }
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.as_object().cloned())
}

fn extract_json_str(raw: &str, keys: &[&str]) -> Option<String> {
    let obj = parse_json_object(raw)?;
    for key in keys {
        if let Some(s) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_screen_commands() {
        assert_eq!(
            MqttCommandParser::parse("iris/tablet-1/command/screen", "next"),
            Some(IrisCommand::NextScreen)
        );
        assert_eq!(
            MqttCommandParser::parse(
                "iris/tablet-1/command/screen",
                r#"{"screen":"o2-overview"}"#
            ),
            Some(IrisCommand::SwitchScreen {
                target: "o2-overview".to_string()
            })
        );
    }

    #[test]
    fn parses_alert_trigger_and_clear() {
        let cmd = MqttCommandParser::parse(
            "iris/tablet-1/command/alert",
            r#"{"severity":"critical","title":"Pod CrashLoop","message":"o2-ingester","dashboardUrl":"https://o2-openobserve.p.zacharie.org/web/dashboards","source":"openobserve"}"#,
        );
        match cmd {
            Some(IrisCommand::TriggerAlert(alert)) => {
                assert_eq!(alert.severity, AlertSeverity::Critical);
                assert_eq!(alert.title, "Pod CrashLoop");
                assert_eq!(alert.source, "openobserve");
            }
            other => panic!("unexpected command: {other:?}"),
        }

        assert_eq!(
            MqttCommandParser::parse("iris/tablet-1/command/alert", "resolved"),
            Some(IrisCommand::ClearAlert)
        );
        assert_eq!(
            MqttCommandParser::parse("iris/tablet-1/command/alert", r#"{"active":false}"#),
            Some(IrisCommand::ClearAlert)
        );
    }

    #[test]
    fn infers_provider_from_url() {
        let cmd = MqttCommandParser::parse(
            "iris/tablet-1/command/url",
            "https://grafana.zacharie.org/d/xyz",
        );
        assert_eq!(
            cmd,
            Some(IrisCommand::OpenUrl {
                url: "https://grafana.zacharie.org/d/xyz".to_string(),
                title: "Écran MQTT".to_string(),
                provider: ScreenType::Grafana,
            })
        );
    }

    #[test]
    fn parses_brightness_and_rotation() {
        assert_eq!(
            MqttCommandParser::parse("iris/tablet-1/command/brightness", "dim"),
            Some(IrisCommand::SetBrightness { percent: 15 })
        );
        assert_eq!(
            MqttCommandParser::parse(
                "iris/tablet-1/command/rotation",
                r#"{"enabled":true,"intervalSeconds":60}"#
            ),
            Some(IrisCommand::SetRotation {
                enabled: true,
                interval_seconds: Some(60)
            })
        );
    }
}
