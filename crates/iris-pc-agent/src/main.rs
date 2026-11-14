use std::env;
use std::process::Command;
use std::time::Duration;

use chrono::Utc;
use iris_core::{DeckActionEvent, DeckActionKind, IrisSnapshot, PcTelemetry};
use sysinfo::System;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let server_url = env::var("IRIS_SERVER_URL")
        .unwrap_or_else(|_| "https://iris.zacharie.org".to_string())
        .trim_end_matches('/')
        .to_string();
    let token_limit = env::var("ANTIGRAVITY_TOKEN_LIMIT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2_000_000);
    let active_model =
        env::var("ANTIGRAVITY_MODEL").unwrap_or_else(|_| "Gemini 4 Hydrogen".to_string());

    info!(%server_url, "Iris PC Agent started");

    let http = reqwest::Client::new();
    let mut sys = System::new_all();
    let mut last_action_id = String::new();
    let mut tick: u32 = 0;

    loop {
        if tick.is_multiple_of(6) {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            let cpu_percent = sys.global_cpu_usage().round() as u32;
            let ram_total_mb = sys.total_memory() / (1024 * 1024);
            let ram_used_mb = sys.used_memory() / (1024 * 1024);
            let ram_percent = if ram_total_mb > 0 {
                ((ram_used_mb * 100) / ram_total_mb) as u32
            } else {
                0
            };
            let active_meeting = detect_active_meeting(&sys);
            let hostname = System::host_name().unwrap_or_else(|| "Windows-PC".to_string());

            let telemetry = PcTelemetry {
                hostname,
                cpu_percent,
                ram_used_mb,
                ram_total_mb,
                ram_percent,
                uptime_secs: System::uptime(),
                active_meeting,
                antigravity_tokens_used: read_antigravity_tokens_used(),
                antigravity_tokens_limit: token_limit,
                antigravity_model: active_model.clone(),
                updated_at_epoch_ms: Utc::now().timestamp_millis(),
            };

            if let Err(err) = http
                .post(format!("{server_url}/api/pc/telemetry"))
                .json(&telemetry)
                .send()
                .await
            {
                warn!(%err, "Failed to push PC telemetry");
            }
        }

        if let Ok(resp) = http.get(format!("{server_url}/api/state")).send().await {
            if let Ok(snap) = resp.json::<IrisSnapshot>().await {
                if let Some(action) = snap.last_deck_action {
                    if !last_action_id.is_empty() && action.id != last_action_id {
                        execute_deck_action(&action);
                    }
                    last_action_id = action.id;
                } else if last_action_id.is_empty() {
                    last_action_id = "init".to_string();
                }
            }
        }

        tick = tick.wrapping_add(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn detect_active_meeting(sys: &System) -> Option<String> {
    for proc_info in sys.processes().values() {
        let name = proc_info.name().to_string_lossy().to_lowercase();
        if name.contains("ms-teams") || name == "teams.exe" {
            return Some("Teams".to_string());
        }
        if name.contains("zoom.exe") || name == "zoom" {
            return Some("Zoom".to_string());
        }
    }
    None
}

fn read_antigravity_tokens_used() -> u64 {
    env::var("ANTIGRAVITY_TOKENS_USED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn execute_deck_action(action: &DeckActionEvent) {
    info!(label = %action.label, kind = ?action.kind, payload = %action.payload, "Executing Stream Deck action");

    match action.kind {
        DeckActionKind::JoinTeams => {
            let target = if action.payload.trim().is_empty() {
                "msteams://"
            } else {
                action.payload.trim()
            };
            open_shell_target(target);
        }
        DeckActionKind::JoinZoom => {
            let target = if action.payload.trim().is_empty() {
                "zoommtg://"
            } else {
                action.payload.trim()
            };
            open_shell_target(target);
        }
        DeckActionKind::OpenUrl => {
            if !action.payload.trim().is_empty() {
                open_shell_target(action.payload.trim());
            }
        }
        DeckActionKind::LaunchApp => {
            let app = action.payload.trim();
            if !app.is_empty() {
                let _ = Command::new("cmd").args(["/C", "start", "", app]).spawn();
            }
        }
        DeckActionKind::MediaToggle => {
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-Command",
                        "(New-Object -ComObject WScript.Shell).SendKeys([char]179)",
                    ])
                    .spawn();
            }
        }
        DeckActionKind::LockScreen => {
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("rundll32.exe")
                    .args(["user32.dll,LockWorkStation"])
                    .spawn();
            }
        }
        DeckActionKind::SwitchIrisScreen => {}
    }
}

fn open_shell_target(target: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("xdg-open").arg(target).spawn();
    }
}
