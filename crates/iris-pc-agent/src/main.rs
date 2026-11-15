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

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default();
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
            let ram_percent = (ram_used_mb * 100).checked_div(ram_total_mb).unwrap_or(0) as u32;
            let (gpu_name, gpu_percent, gpu_mem_used_mb, gpu_mem_total_mb, gpu_mem_percent) =
                query_nvidia_gpu();
            let active_meeting = detect_active_meeting(&sys);
            let hostname = System::host_name().unwrap_or_else(|| "Windows-PC".to_string());

            let telemetry = PcTelemetry {
                hostname,
                cpu_percent,
                ram_used_mb,
                ram_total_mb,
                ram_percent,
                gpu_name,
                gpu_percent,
                gpu_mem_used_mb,
                gpu_mem_total_mb,
                gpu_mem_percent,
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
                        execute_deck_action(&action, &http, &server_url).await;
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

fn query_nvidia_gpu() -> (String, u32, u64, u64, u32) {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,utilization.gpu,memory.used,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = cmd.output() else {
        return (String::new(), 0, 0, 0, 0);
    };
    if !output.status.success() {
        return (String::new(), 0, 0, 0, 0);
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let first_line = line.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return (String::new(), 0, 0, 0, 0);
    }
    let parts: Vec<&str> = first_line.split(',').map(str::trim).collect();
    if parts.len() < 4 {
        return (String::new(), 0, 0, 0, 0);
    }
    let name = parts[0].to_string();
    let util = parts[1].parse::<u32>().unwrap_or(0);
    let mem_used = parts[2].parse::<u64>().unwrap_or(0);
    let mem_total = parts[3].parse::<u64>().unwrap_or(0);
    let mem_pct = (mem_used * 100).checked_div(mem_total).unwrap_or(0) as u32;
    (name, util, mem_used, mem_total, mem_pct)
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

async fn execute_deck_action(action: &DeckActionEvent, http: &reqwest::Client, server_url: &str) {
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
        DeckActionKind::UpdateUniGet => {
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("cmd")
                    .args([
                        "/C",
                        "start",
                        "UniGetUI Upgrade",
                        "powershell",
                        "-NoProfile",
                        "-Command",
                        "winget upgrade --all --include-unknown --accept-package-agreements --accept-source-agreements",
                    ])
                    .spawn();
            }
        }
        DeckActionKind::UpdateProxmoxVms => {
            let _ = http
                .post(format!("{server_url}/api/infra/proxmox/update"))
                .send()
                .await;
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
