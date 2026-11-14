use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use iris_application::IrisController;
use iris_core::{ActiveAlert, AlertSeverity, DisplayScreen, IrisSettings, IrisSnapshot};
use iris_infrastructure::RumqttcAdapter;
use prometheus::{Encoder, TextEncoder};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub controller: Arc<IrisController>,
    pub mqtt: Arc<RumqttcAdapter>,
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/metrics", get(metrics))
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
        .route("/api/settings", post(update_settings))
        .route("/api/reload", post(request_reload))
        .route("/ws", get(ws_handler))
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
    Json(state.controller.upsert_screen(screen).await)
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

async fn update_settings(
    State(state): State<AppState>,
    Json(new_settings): Json<IrisSettings>,
) -> Json<IrisSnapshot> {
    let old_mqtt = state.controller.snapshot().await.settings.mqtt;
    let reconnect = old_mqtt != new_settings.mqtt;
    let mqtt_cfg = new_settings.mqtt.clone();
    let snap = state
        .controller
        .update_settings(|s| *s = new_settings.clone())
        .await;
    if reconnect {
        state.mqtt.connect(mqtt_cfg).await;
    }
    Json(snap)
}

async fn request_reload(State(state): State<AppState>) -> Json<IrisSnapshot> {
    Json(state.controller.request_reload().await)
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
