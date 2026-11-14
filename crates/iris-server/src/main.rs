mod assets;
mod config;
mod routes;

use std::sync::Arc;

use iris_application::{IrisController, StateRepository};
use iris_infrastructure::{JsonFileRepository, RumqttcAdapter};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::assets::serve_static;
use crate::config::ServerConfig;
use crate::routes::{api_router, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("iris=info,info")),
        )
        .init();

    let cfg = ServerConfig::from_env();
    let repo = Arc::new(JsonFileRepository::new(&cfg.state_file));
    let mut initial = repo.load().await?;

    if let Some(host) = cfg.mqtt_host_override {
        initial.settings.mqtt.broker_host = host;
    }
    if let Some(port) = cfg.mqtt_port_override {
        initial.settings.mqtt.broker_port = port;
    }

    let controller = IrisController::new(initial.clone(), repo);
    controller.spawn_rotation_loop();

    let mqtt = RumqttcAdapter::new(Arc::clone(&controller));
    controller
        .set_mqtt_publisher(Arc::clone(&mqtt) as Arc<dyn iris_application::MqttPublisher>)
        .await;
    mqtt.connect(initial.settings.mqtt).await;

    let app_state = AppState { controller, mqtt };

    let app = api_router(app_state)
        .fallback(serve_static)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&cfg.listen_addr).await?;
    info!(addr = %cfg.listen_addr, "Iris Rust server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
