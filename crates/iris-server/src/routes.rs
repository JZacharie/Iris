use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use iris_application::IrisController;
use iris_core::{
    ActiveAlert, AgentUpgradeReport, AlertSeverity, CommsStatus, DeckButton, DisplayScreen,
    DriveOrder, IrisSettings, IrisSnapshot, OrgEvent, OrgMail, PcTelemetry, ScreenType,
    ShoppingItem,
};
use iris_infrastructure::{
    AmarakiClient, DashboardRenderer, EnderClient, ParsedVoiceAction, RumqttcAdapter, SlackClient,
    VoiceIntentEngine,
};
use prometheus::{Encoder, TextEncoder};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub controller: Arc<IrisController>,
    pub mqtt: Arc<RumqttcAdapter>,
    pub renderer: Arc<DashboardRenderer>,
    pub ender: Arc<EnderClient>,
    pub slack: Arc<SlackClient>,
    pub amaraki: Arc<AmarakiClient>,
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/renders/{id}", get(get_screen_render))
        .route("/api/renders/{id}/refresh", post(refresh_screen_render))
        .route("/api/renders/{id}/meta", get(get_render_meta))
        .route("/api/state", get(get_state))
        .route("/api/screen/select", post(select_screen))
        .route("/api/screen/next", post(next_screen))
        .route("/api/screen/prev", post(prev_screen))
        .route("/api/screens", post(upsert_screen))
        .route("/api/screens/{id}", delete(delete_screen))
        .route("/api/alert/trigger", post(trigger_alert))
        .route("/api/alert/ack", post(ack_alert))
        .route("/api/alert/clear", post(clear_alert))
        .route("/api/webhooks/openobserve", post(webhook_openobserve))
        .route("/api/webhooks/grafana", post(webhook_grafana))
        .route("/api/notes", post(upsert_note))
        .route("/api/notes/{id}/toggle", post(toggle_note))
        .route("/api/notes/{id}", delete(delete_note))
        .route("/api/org/refresh", post(refresh_org))
        .route("/api/org/mails", post(upsert_mail))
        .route("/api/org/mails/{id}", delete(delete_mail))
        .route("/api/org/events", post(upsert_event))
        .route("/api/org/events/{id}", delete(delete_event))
        .route("/api/comms/status", post(update_comms_status))
        .route("/api/home/shopping", post(upsert_shopping_item))
        .route("/api/home/shopping/{id}/toggle", post(toggle_shopping_item))
        .route("/api/home/shopping/{id}", delete(delete_shopping_item))
        .route("/api/home/cleardrive", post(upsert_drive_order))
        .route("/api/home/cleardrive/{id}", delete(delete_drive_order))
        .route("/api/pc/telemetry", post(update_pc_telemetry))
        .route("/api/infra/proxmox/update", post(update_proxmox_vms))
        .route(
            "/api/agents/reports",
            get(get_agent_reports).post(post_agent_report),
        )
        .route("/api/agents/trigger", post(trigger_amaraki_agent))
        .route("/api/voice/intent", post(post_voice_intent))
        .route("/api/printer3d/status", get(get_printer3d_status))
        .route(
            "/api/printer3d/camera/snapshot",
            get(get_printer3d_camera_snapshot),
        )
        .route("/api/printer3d/job", post(post_printer3d_job))
        .route(
            "/api/printer3d/files/select",
            post(post_printer3d_select_file),
        )
        .route("/api/printer3d/power/on", post(power_on_printer3d))
        .route("/api/printer3d/power/off", post(power_off_printer3d))
        .route("/api/printer3d/cooldown", post(cooldown_printer3d))
        .route(
            "/api/printer3d/wake-and-print",
            post(wake_and_print_printer3d),
        )
        .route("/api/cluster/diagnose", post(trigger_cluster_diagnose))
        .route("/api/cluster/remediate", post(trigger_cluster_remediate))
        .route("/api/deck/trigger", post(trigger_deck_button))
        .route("/api/deck/buttons", post(upsert_deck_button))
        .route("/api/deck/buttons/{id}", delete(delete_deck_button))
        .route("/api/settings", post(update_settings))
        .route("/api/reload", post(request_reload))
        .route("/ws", get(ws_handler))
        .route("/ws/pc", get(ws_pc_handler))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "iris" }))
}

async fn metrics() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "metrics error").into_response();
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, encoder.format_type().to_string())],
        buffer,
    )
        .into_response()
}

async fn get_state(State(state): State<AppState>) -> Json<IrisSnapshot> {
    Json(state.controller.snapshot().await)
}

#[derive(Deserialize)]
struct SelectScreenReq {
    target: String,
}

async fn select_screen(
    State(state): State<AppState>,
    Json(req): Json<SelectScreenReq>,
) -> impl IntoResponse {
    match state.controller.select_screen(&req.target).await {
        Ok(snap) => (StatusCode::OK, Json(snap)).into_response(),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn next_screen(State(state): State<AppState>) -> impl IntoResponse {
    match state.controller.next_screen().await {
        Ok(snap) => (StatusCode::OK, Json(snap)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn prev_screen(State(state): State<AppState>) -> impl IntoResponse {
    match state.controller.previous_screen().await {
        Ok(snap) => (StatusCode::OK, Json(snap)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn upsert_screen(
    State(state): State<AppState>,
    Json(screen): Json<DisplayScreen>,
) -> Json<IrisSnapshot> {
    let screen_type = screen.screen_type;
    let screen_id = screen.id.clone();
    let screen_url = screen.url.clone();
    let snap = state.controller.upsert_screen(screen).await;

    if matches!(screen_type, ScreenType::Grafana | ScreenType::OpenObserve) {
        let renderer = state.renderer.clone();
        tokio::spawn(async move {
            let _ = renderer
                .render_and_cache(&screen_id, screen_type, &screen_url)
                .await;
        });
    }

    Json(snap)
}

async fn delete_screen(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<IrisSnapshot> {
    Json(state.controller.delete_screen(&id).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TriggerAlertReq {
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default = "default_critical")]
    severity: AlertSeverity,
    #[serde(default = "default_api_source")]
    source: String,
    #[serde(default)]
    dashboard_url: Option<String>,
    #[serde(default)]
    screen_id: Option<String>,
}

fn default_critical() -> AlertSeverity {
    AlertSeverity::Critical
}
fn default_api_source() -> String {
    "api".to_string()
}

async fn trigger_alert(
    State(state): State<AppState>,
    Json(req): Json<TriggerAlertReq>,
) -> Json<IrisSnapshot> {
    let alert = ActiveAlert {
        id: format!("alert-{}", Uuid::new_v4()),
        severity: req.severity,
        title: req.title,
        message: req.message,
        source: req.source,
        dashboard_url: req.dashboard_url,
        screen_id: req.screen_id,
        triggered_at_epoch_ms: Utc::now().timestamp_millis(),
        acknowledged: false,
    };
    Json(state.controller.trigger_alert(alert).await)
}

async fn ack_alert(State(state): State<AppState>) -> Json<IrisSnapshot> {
    Json(state.controller.acknowledge_alert().await)
}

async fn clear_alert(State(state): State<AppState>) -> Json<IrisSnapshot> {
    Json(state.controller.clear_alert().await)
}

async fn webhook_openobserve(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Json<IrisSnapshot> {
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("firing")
        .to_lowercase();
    if status == "resolved" || status == "ok" {
        return Json(state.controller.clear_alert().await);
    }

    let title = payload
        .get("alert_name")
        .or_else(|| payload.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Alerte OpenObserve")
        .to_string();
    let message = payload
        .get("message")
        .or_else(|| payload.get("alert_url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let dashboard_url = payload
        .get("dashboard_url")
        .or_else(|| payload.get("alert_url"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let alert = ActiveAlert {
        id: format!("o2-{}", Uuid::new_v4()),
        severity: AlertSeverity::Critical,
        title,
        message,
        source: "openobserve".to_string(),
        dashboard_url,
        screen_id: None,
        triggered_at_epoch_ms: Utc::now().timestamp_millis(),
        acknowledged: false,
    };
    Json(state.controller.trigger_alert(alert).await)
}

async fn webhook_grafana(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Json<IrisSnapshot> {
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("firing")
        .to_lowercase();
    if status == "resolved" {
        return Json(state.controller.clear_alert().await);
    }

    let title = payload
        .get("title")
        .or_else(|| payload.get("commonLabels").and_then(|l| l.get("alertname")))
        .and_then(Value::as_str)
        .unwrap_or("Alerte Grafana")
        .to_string();
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let dashboard_url = payload
        .get("externalURL")
        .or_else(|| payload.get("dashboardURL"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let alert = ActiveAlert {
        id: format!("grafana-{}", Uuid::new_v4()),
        severity: AlertSeverity::Critical,
        title,
        message,
        source: "grafana".to_string(),
        dashboard_url,
        screen_id: None,
        triggered_at_epoch_ms: Utc::now().timestamp_millis(),
        acknowledged: false,
    };
    Json(state.controller.trigger_alert(alert).await)
}

#[derive(Deserialize)]
struct UpsertNoteReq {
    id: Option<String>,
    text: String,
    #[serde(default = "default_cat")]
    category: String,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    pinned: bool,
}

fn default_cat() -> String {
    "Organisation".to_string()
}

async fn upsert_note(
    State(state): State<AppState>,
    Json(req): Json<UpsertNoteReq>,
) -> Json<IrisSnapshot> {
    Json(
        state
            .controller
            .upsert_note(req.id, req.text, req.category, req.done, req.pinned)
            .await,
    )
}

async fn toggle_note(State(state): State<AppState>, Path(id): Path<String>) -> Json<IrisSnapshot> {
    Json(state.controller.toggle_note_done(&id).await)
}

async fn delete_note(State(state): State<AppState>, Path(id): Path<String>) -> Json<IrisSnapshot> {
    Json(state.controller.delete_note(&id).await)
}

async fn refresh_org(State(state): State<AppState>) -> Json<IrisSnapshot> {
    crate::org_sync::refresh_org_now(Arc::clone(&state.controller)).await;
    Json(state.controller.snapshot().await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertMailReq {
    id: Option<String>,
    sender: String,
    subject: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    received_at: String,
    #[serde(default = "default_true")]
    unread: bool,
    #[serde(default)]
    important: bool,
}

fn default_true() -> bool {
    true
}

async fn upsert_mail(
    State(state): State<AppState>,
    Json(req): Json<UpsertMailReq>,
) -> Json<IrisSnapshot> {
    let mail = OrgMail {
        id: req.id.unwrap_or_else(|| format!("mail-{}", Uuid::new_v4())),
        sender: req.sender,
        subject: req.subject,
        snippet: req.snippet,
        received_at: if req.received_at.is_empty() {
            Utc::now().format("%H:%M").to_string()
        } else {
            req.received_at
        },
        unread: req.unread,
        important: req.important,
    };
    Json(state.controller.upsert_mail(mail).await)
}

async fn delete_mail(State(state): State<AppState>, Path(id): Path<String>) -> Json<IrisSnapshot> {
    Json(state.controller.delete_mail(&id).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertEventReq {
    id: Option<String>,
    title: String,
    date: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    end_time: String,
    #[serde(default)]
    location: String,
}

async fn upsert_event(
    State(state): State<AppState>,
    Json(req): Json<UpsertEventReq>,
) -> Json<IrisSnapshot> {
    let event = OrgEvent {
        id: req.id.unwrap_or_else(|| format!("evt-{}", Uuid::new_v4())),
        title: req.title,
        date: req.date,
        start_time: req.start_time,
        end_time: req.end_time,
        location: req.location,
        source: "manual".to_string(),
    };
    Json(state.controller.upsert_event(event).await)
}

async fn delete_event(State(state): State<AppState>, Path(id): Path<String>) -> Json<IrisSnapshot> {
    Json(state.controller.delete_event(&id).await)
}

async fn update_pc_telemetry(
    State(state): State<AppState>,
    Json(telemetry): Json<PcTelemetry>,
) -> Json<IrisSnapshot> {
    Json(state.controller.update_pc_telemetry(telemetry).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TriggerDeckReq {
    button_id: String,
}

async fn trigger_deck_button(
    State(state): State<AppState>,
    Json(req): Json<TriggerDeckReq>,
) -> impl IntoResponse {
    match state.controller.trigger_deck_button(&req.button_id).await {
        Ok(snap) => {
            if snap
                .last_deck_action
                .as_ref()
                .is_some_and(|a| a.kind == iris_core::DeckActionKind::UpdateProxmoxVms)
            {
                tokio::spawn(async {
                    let _ = iris_infrastructure::kusanagi_client::trigger_proxmox_updates().await;
                });
            }
            (StatusCode::OK, Json(snap)).into_response()
        }
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn update_proxmox_vms(State(state): State<AppState>) -> impl IntoResponse {
    match iris_infrastructure::kusanagi_client::trigger_proxmox_updates().await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "updatedTargets": count,
                "snapshot": state.controller.snapshot().await
            })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_agent_reports(State(state): State<AppState>) -> Json<Vec<AgentUpgradeReport>> {
    Json(state.controller.snapshot().await.agent_reports)
}

async fn post_agent_report(
    State(state): State<AppState>,
    Json(mut report): Json<AgentUpgradeReport>,
) -> Json<IrisSnapshot> {
    if report.updated_at_epoch_ms == 0 {
        report.updated_at_epoch_ms = Utc::now().timestamp_millis();
    }
    Json(state.controller.upsert_agent_report(report).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TriggerAmarakiReq {
    #[serde(alias = "agent_name")]
    agent_name: String,
    prompt: String,
    #[serde(default)]
    channel: Option<String>,
}

async fn trigger_amaraki_agent(
    State(state): State<AppState>,
    Json(req): Json<TriggerAmarakiReq>,
) -> impl IntoResponse {
    let agent_name = req.agent_name.trim();
    let prompt = req.prompt.trim();
    if agent_name.is_empty() || prompt.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "agentName et prompt sont requis"
            })),
        )
            .into_response();
    }

    let client = AmarakiClient::from_env();
    match client
        .trigger_agent(agent_name, prompt, req.channel.as_deref())
        .await
    {
        Ok(job_id) => {
            let now = Utc::now();
            let report = AgentUpgradeReport {
                id: job_id.clone(),
                agent_name: agent_name.to_string(),
                model: String::new(),
                channel: req.channel.unwrap_or_else(|| "#ai".to_string()),
                prompt: prompt.to_string(),
                status: "running".to_string(),
                is_real_success: true,
                summary_note: "Mission déclenchée depuis Iris (en cours...)".to_string(),
                report_output: String::new(),
                started_at: now.to_rfc3339(),
                completed_at: None,
                duration_secs: None,
                updated_at_epoch_ms: now.timestamp_millis(),
            };
            let snap = state.controller.upsert_agent_report(report).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "spawned",
                    "jobId": job_id,
                    "snapshot": snap
                })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct VoiceIntentPayload {
    text: String,
}

async fn post_voice_intent(
    State(state): State<AppState>,
    Json(payload): Json<VoiceIntentPayload>,
) -> impl IntoResponse {
    let (result, action) = VoiceIntentEngine::parse(&payload.text);

    match &action {
        ParsedVoiceAction::NavigateScreen(screen_id) => {
            let _ = state.controller.select_screen(screen_id).await;
        }
        ParsedVoiceAction::TriggerAgent { agent, prompt } => {
            let _ = state.controller.select_screen("org-agents").await;
            let amaraki = Arc::clone(&state.amaraki);
            let ag = agent.clone();
            let pr = prompt.clone();
            tokio::spawn(async move {
                let _ = amaraki.trigger_agent(&ag, &pr, Some("#ai")).await;
            });
        }
        ParsedVoiceAction::Printer3DPowerOn => {
            let _ = state.controller.select_screen("org-3d-print").await;
            let ender = Arc::clone(&state.ender);
            tokio::spawn(async move {
                let _ = ender.power_on().await;
            });
        }
        ParsedVoiceAction::Printer3DCooldown => {
            let _ = state.controller.select_screen("org-3d-print").await;
            let ender = Arc::clone(&state.ender);
            tokio::spawn(async move {
                let _ = ender.cooldown_and_off().await;
            });
        }
        ParsedVoiceAction::None => {}
    }

    let _snap = state.controller.record_voice_intent(result.clone()).await;

    let slack = Arc::clone(&state.slack);
    let slack_msg = format!(
        "🎙️ *Commande Vocale Iris (Tablette)*\n• *Joe* : {}\n• *Assistant* : {}\n• _Action_ : `{}`",
        payload.text, result.spoken_reply, result.intent_type
    );
    tokio::spawn(async move {
        let _ = slack.post_message(&slack_msg).await;
    });

    Json(result)
}

async fn get_printer3d_status(State(state): State<AppState>) -> impl IntoResponse {
    match state.ender.fetch_status().await {
        Ok(status) => {
            state
                .controller
                .apply_printer3d_telemetry(Some(status.clone()))
                .await;
            Json(serde_json::to_value(status).unwrap_or_default())
        }
        Err(e) => Json(serde_json::json!({
            "connected": false,
            "error": e.to_string(),
        })),
    }
}

async fn power_on_printer3d(State(state): State<AppState>) -> impl IntoResponse {
    let ok = state.ender.power_on().await.unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

#[derive(Deserialize, Default)]
struct PowerOffQuery {
    #[serde(default)]
    force: bool,
}

async fn power_off_printer3d(
    State(state): State<AppState>,
    Json(payload): Json<Option<PowerOffQuery>>,
) -> impl IntoResponse {
    let force = payload.map(|p| p.force).unwrap_or(false);
    let ok = state.ender.power_off(force).await.unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

async fn cooldown_printer3d(State(state): State<AppState>) -> impl IntoResponse {
    let ok = state.ender.cooldown_and_off().await.unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

async fn wake_and_print_printer3d(State(state): State<AppState>) -> impl IntoResponse {
    let ok = state.ender.wake_and_print().await.unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

async fn get_printer3d_camera_snapshot(State(state): State<AppState>) -> impl IntoResponse {
    match state.ender.fetch_webcam_snapshot().await {
        Ok((bytes, content_type)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store, max-age=0".to_string()),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Printer3DJobReq {
    action: String,
}

async fn post_printer3d_job(
    State(state): State<AppState>,
    Json(req): Json<Printer3DJobReq>,
) -> impl IntoResponse {
    let ok = state.ender.job_command(&req.action).await.unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Printer3DSelectFileReq {
    path: String,
    #[serde(default)]
    print: bool,
}

async fn post_printer3d_select_file(
    State(state): State<AppState>,
    Json(req): Json<Printer3DSelectFileReq>,
) -> impl IntoResponse {
    let ok = state
        .ender
        .select_file(&req.path, req.print)
        .await
        .unwrap_or(false);
    if let Ok(st) = state.ender.fetch_status().await {
        state.controller.apply_printer3d_telemetry(Some(st)).await;
    }
    Json(serde_json::json!({ "success": ok }))
}

async fn trigger_cluster_diagnose(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.controller.select_screen("org-agents").await;
    let res = state
        .amaraki
        .trigger_agent(
            "agent-k8s-diagnostician",
            "Diagnostic approfondi des pods, anomalies et santé du cluster jo3.",
            Some("#ai"),
        )
        .await;
    let slack = Arc::clone(&state.slack);
    tokio::spawn(async move {
        let _ = slack
            .post_message(
                "☸️ *Audit K8s jo3 déclenché depuis Iris* : Mission confiée à `agent-k8s-diagnostician`.",
            )
            .await;
    });
    match res {
        Ok(job_id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "jobId": job_id })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn trigger_cluster_remediate(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.controller.select_screen("org-agents").await;
    let res = state
        .amaraki
        .trigger_agent(
            "opencode-cluster-remediator",
            "Analyse les dérives GitOps et applications ArgoCD dégradées, puis applique les remédiations.",
            Some("#ai"),
        )
        .await;
    let slack = Arc::clone(&state.slack);
    tokio::spawn(async move {
        let _ = slack
            .post_message(
                "🛠️ *Remédiation GitOps jo3 déclenchée depuis Iris* : Mission confiée à `opencode-cluster-remediator`.",
            )
            .await;
    });
    match res {
        Ok(job_id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "jobId": job_id })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn upsert_deck_button(
    State(state): State<AppState>,
    Json(button): Json<DeckButton>,
) -> Json<IrisSnapshot> {
    Json(state.controller.upsert_deck_button(button).await)
}

async fn delete_deck_button(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<IrisSnapshot> {
    Json(state.controller.delete_deck_button(&id).await)
}

async fn ws_pc_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_pc(socket, state))
}

async fn handle_ws_pc(mut socket: WebSocket, state: AppState) {
    let mut rx = state.controller.subscribe_deck_actions();
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(action) => {
                        if let Ok(json) = serde_json::to_string(&action) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                if incoming.is_none() || matches!(incoming, Some(Ok(Message::Close(_)))) {
                    break;
                }
            }
        }
    }
}

async fn update_settings(
    State(state): State<AppState>,
    Json(new_settings): Json<IrisSettings>,
) -> Json<IrisSnapshot> {
    let current = state.controller.snapshot().await.settings;
    let reconnect = current.mqtt != new_settings.mqtt;
    let refresh_org_needed = current.org != new_settings.org;
    let mqtt_cfg = new_settings.mqtt.clone();
    let snap = state
        .controller
        .update_settings(|s| *s = new_settings.clone())
        .await;
    if reconnect {
        state.mqtt.connect(mqtt_cfg).await;
    }
    if refresh_org_needed {
        let ctrl = Arc::clone(&state.controller);
        tokio::spawn(async move {
            crate::org_sync::refresh_org_now(ctrl).await;
        });
    }
    Json(snap)
}

async fn request_reload(State(state): State<AppState>) -> Json<IrisSnapshot> {
    Json(state.controller.request_reload().await)
}

async fn update_comms_status(
    State(state): State<AppState>,
    Json(mut comms): Json<CommsStatus>,
) -> Json<IrisSnapshot> {
    if comms.updated_at_epoch_ms == 0 {
        comms.updated_at_epoch_ms = Utc::now().timestamp_millis();
    }
    Json(state.controller.update_comms(comms).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertShoppingItemReq {
    id: Option<String>,
    name: String,
    #[serde(default)]
    completed: bool,
}

async fn upsert_shopping_item(
    State(state): State<AppState>,
    Json(req): Json<UpsertShoppingItemReq>,
) -> Json<IrisSnapshot> {
    let item = ShoppingItem {
        id: req.id.unwrap_or_else(|| format!("shop-{}", Uuid::new_v4())),
        name: req.name,
        completed: req.completed,
    };
    Json(state.controller.upsert_shopping_item(item).await)
}

async fn toggle_shopping_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<IrisSnapshot> {
    Json(state.controller.toggle_shopping_item(&id).await)
}

async fn delete_shopping_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<IrisSnapshot> {
    Json(state.controller.delete_shopping_item(&id).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertDriveOrderReq {
    id: Option<String>,
    #[serde(default = "default_drive_store")]
    store: String,
    status: String,
    #[serde(default)]
    pickup_slot: String,
    #[serde(default)]
    ready_for_pickup: bool,
}

fn default_drive_store() -> String {
    "ClearDrive".to_string()
}

async fn upsert_drive_order(
    State(state): State<AppState>,
    Json(req): Json<UpsertDriveOrderReq>,
) -> Json<IrisSnapshot> {
    let order = DriveOrder {
        id: req
            .id
            .unwrap_or_else(|| format!("drive-{}", Uuid::new_v4())),
        store: req.store,
        status: req.status,
        pickup_slot: req.pickup_slot,
        ready_for_pickup: req.ready_for_pickup,
    };
    Json(state.controller.upsert_drive_order(order).await)
}

async fn delete_drive_order(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<IrisSnapshot> {
    Json(state.controller.delete_drive_order(&id).await)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let initial = state.controller.snapshot().await;
    if let Ok(json) = serde_json::to_string(&initial) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    let mut rx = state.controller.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(snap) => {
                        if let Ok(json) = serde_json::to_string(&snap) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                if incoming.is_none() || matches!(incoming, Some(Ok(Message::Close(_)))) {
                    break;
                }
            }
        }
    }
}

#[derive(Deserialize, Default)]
struct RenderQuery {
    #[serde(default)]
    force: bool,
    w: Option<u32>,
    h: Option<u32>,
}

async fn get_screen_render(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<RenderQuery>,
) -> impl IntoResponse {
    let snap = state.controller.snapshot().await;
    let screen = snap.screens.iter().find(|s| s.id == id).cloned().or_else(|| {
        if id == "grafana" || id.contains("grafana") {
            Some(DisplayScreen {
                id: id.clone(),
                name: "Grafana".to_string(),
                screen_type: ScreenType::Grafana,
                url: "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 60,
                enabled: true,
                is_alert_target: false,
            })
        } else if id == "openobserve" || id.contains("openobserve") || id.contains("o2") {
            Some(DisplayScreen {
                id: id.clone(),
                name: "OpenObserve".to_string(),
                screen_type: ScreenType::OpenObserve,
                url: "7516898066293608448".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 60,
                enabled: true,
                is_alert_target: false,
            })
        } else {
            None
        }
    });

    let screen = match screen {
        Some(s) => s,
        None => {
            let svg = DashboardRenderer::generate_fallback_svg(&id, "Écran introuvable");
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "image/svg+xml")],
                svg,
            )
                .into_response();
        }
    };

    let target_url = if screen.url.trim().is_empty() {
        &snap.formatted_url
    } else {
        &screen.url
    };

    if !query.force {
        if let Some(entry) = state
            .renderer
            .get_cached_for_screen(&id, screen.screen_type, target_url, query.w, query.h)
            .await
        {
            let rendered_at_str = entry.rendered_at.to_rfc3339();
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=15"),
                    (
                        header::HeaderName::from_static("x-iris-rendered-at"),
                        rendered_at_str.as_str(),
                    ),
                ],
                entry.image_png,
            )
                .into_response();
        }
    }

    match state
        .renderer
        .render_and_cache_sized(&screen.id, screen.screen_type, target_url, query.w, query.h)
        .await
    {
        Ok(entry) => {
            let rendered_at_str = entry.rendered_at.to_rfc3339();
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=15"),
                    (
                        header::HeaderName::from_static("x-iris-rendered-at"),
                        rendered_at_str.as_str(),
                    ),
                ],
                entry.image_png,
            )
                .into_response()
        }
        Err(err) => {
            tracing::error!(screen = %screen.name, error = %err, "Failed to render screen");
            if let Some(stale) = state.renderer.get_cached(&screen.id).await {
                let rendered_at_str = stale.rendered_at.to_rfc3339();
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "image/png"),
                        (header::CACHE_CONTROL, "no-store"),
                        (
                            header::HeaderName::from_static("x-iris-rendered-at"),
                            rendered_at_str.as_str(),
                        ),
                    ],
                    stale.image_png,
                )
                    .into_response();
            }
            let svg = DashboardRenderer::generate_fallback_svg(&screen.name, &err.to_string());
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/svg+xml"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                svg,
            )
                .into_response()
        }
    }
}

async fn refresh_screen_render(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<RenderQuery>,
) -> impl IntoResponse {
    let snap = state.controller.snapshot().await;
    let screen = snap.screens.iter().find(|s| s.id == id).cloned().or_else(|| {
        if id == "grafana" || id.contains("grafana") {
            Some(DisplayScreen {
                id: id.clone(),
                name: "Grafana".to_string(),
                screen_type: ScreenType::Grafana,
                url: "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 60,
                enabled: true,
                is_alert_target: false,
            })
        } else if id == "openobserve" || id.contains("openobserve") || id.contains("o2") {
            Some(DisplayScreen {
                id: id.clone(),
                name: "OpenObserve".to_string(),
                screen_type: ScreenType::OpenObserve,
                url: "7516898066293608448".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 60,
                enabled: true,
                is_alert_target: false,
            })
        } else {
            None
        }
    });

    let screen = match screen {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("screen '{id}' not found") })),
            )
                .into_response();
        }
    };

    let target_url = if screen.url.trim().is_empty() {
        &snap.formatted_url
    } else {
        &screen.url
    };

    match state
        .renderer
        .render_and_cache_sized(&screen.id, screen.screen_type, target_url, query.w, query.h)
        .await
    {
        Ok(entry) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "screen_id": screen.id,
                "width": entry.width,
                "height": entry.height,
                "rendered_at": entry.rendered_at.to_rfc3339(),
                "duration_ms": entry.duration_ms,
                "bytes": entry.image_png.len(),
            })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_render_meta(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(entry) = state.renderer.get_cached(&id).await {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "screen_id": id,
                "cached": true,
                "rendered_at": entry.rendered_at.to_rfc3339(),
                "duration_ms": entry.duration_ms,
                "bytes": entry.image_png.len(),
            })),
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "screen_id": id,
                "cached": false,
            })),
        )
            .into_response()
    }
}
