use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use iris_application::{IrisController, MqttPublisher};
use iris_core::{
    IrisError, IrisSnapshot, MqttCommandParser, MqttConfig, MqttConnectionState, Result,
};
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, QoS};
use serde::Serialize;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedStatePayload {
    device_id: String,
    active_screen_id: Option<String>,
    active_screen_name: Option<String>,
    active_screen_type: Option<String>,
    rotation_enabled: bool,
    rotation_interval_seconds: u32,
    brightness_percent: i32,
    alert_active: bool,
    alert_title: Option<String>,
    timestamp_epoch_ms: i64,
}

struct ActiveMqttHandle {
    client: AsyncClient,
    topic_prefix: String,
    task: JoinHandle<()>,
}

pub struct RumqttcAdapter {
    controller: Arc<IrisController>,
    handle: RwLock<Option<ActiveMqttHandle>>,
}

impl RumqttcAdapter {
    pub fn new(controller: Arc<IrisController>) -> Arc<Self> {
        Arc::new(Self {
            controller,
            handle: RwLock::new(None),
        })
    }

    pub async fn connect(&self, config: MqttConfig) {
        self.disconnect().await;

        if !config.enabled || config.broker_host.trim().is_empty() {
            self.controller
                .update_mqtt_status(MqttConnectionState::Disconnected, "MQTT désactivé")
                .await;
            return;
        }

        let topic_prefix = config.device_topic_prefix();
        let availability_topic = format!("{topic_prefix}/availability");
        let command_topic = format!("{topic_prefix}/command/#");
        let client_id = format!(
            "iris-{}-{}",
            config.normalized_device_id(),
            Utc::now().timestamp_millis() % 10000
        );

        let mut options =
            MqttOptions::new(client_id, config.broker_host.trim(), config.broker_port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);
        options.set_last_will(LastWill::new(
            availability_topic.clone(),
            "offline",
            QoS::AtLeastOnce,
            true,
        ));
        if !config.username.trim().is_empty() {
            options.set_credentials(config.username.trim(), config.password.clone());
        }

        self.controller
            .update_mqtt_status(
                MqttConnectionState::Connecting,
                format!("{}:{}", config.broker_host.trim(), config.broker_port),
            )
            .await;

        let (client, mut eventloop) = AsyncClient::new(options, 32);
        let controller = Arc::clone(&self.controller);
        let client_for_loop = client.clone();

        let task = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                        info!(topic = %command_topic, "Connected to MQTT broker");
                        controller
                            .update_mqtt_status(
                                MqttConnectionState::Connected,
                                format!("{}:{}", config.broker_host.trim(), config.broker_port),
                            )
                            .await;
                        if let Err(err) = client_for_loop
                            .subscribe(command_topic.clone(), QoS::AtLeastOnce)
                            .await
                        {
                            warn!(%err, "Failed to subscribe to MQTT command topic");
                        }
                        let _ = client_for_loop
                            .publish(availability_topic.clone(), QoS::AtLeastOnce, true, "online")
                            .await;
                    }
                    Ok(Event::Incoming(Incoming::Publish(publish))) => {
                        let payload = String::from_utf8_lossy(&publish.payload);
                        if let Some(cmd) = MqttCommandParser::parse(&publish.topic, &payload) {
                            controller.apply_command(cmd).await;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        controller
                            .update_mqtt_status(MqttConnectionState::Error, err.to_string())
                            .await;
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let mut guard = self.handle.write().await;
        *guard = Some(ActiveMqttHandle {
            client,
            topic_prefix,
            task,
        });
    }

    pub async fn disconnect(&self) {
        let existing = {
            let mut guard = self.handle.write().await;
            guard.take()
        };
        if let Some(active) = existing {
            active.task.abort();
            let _ = active.client.disconnect().await;
        }
    }
}

#[async_trait]
impl MqttPublisher for RumqttcAdapter {
    async fn publish_state(&self, snapshot: &IrisSnapshot) -> Result<()> {
        let (client, topic) = {
            let guard = self.handle.read().await;
            let Some(active) = guard.as_ref() else {
                return Ok(());
            };
            (
                active.client.clone(),
                format!("{}/state", active.topic_prefix),
            )
        };

        let payload = PublishedStatePayload {
            device_id: snapshot.settings.mqtt.normalized_device_id(),
            active_screen_id: snapshot.displayed_screen.as_ref().map(|s| s.id.clone()),
            active_screen_name: snapshot.displayed_screen.as_ref().map(|s| s.name.clone()),
            active_screen_type: snapshot
                .displayed_screen
                .as_ref()
                .map(|s| format!("{:?}", s.screen_type)),
            rotation_enabled: snapshot.settings.rotation_enabled,
            rotation_interval_seconds: snapshot.settings.rotation_interval_seconds,
            brightness_percent: snapshot.settings.brightness_percent,
            alert_active: snapshot.active_alert.is_some(),
            alert_title: snapshot.active_alert.as_ref().map(|a| a.title.clone()),
            timestamp_epoch_ms: Utc::now().timestamp_millis(),
        };

        let body = serde_json::to_vec(&payload).map_err(|e| IrisError::Mqtt(e.to_string()))?;
        client
            .publish(topic, QoS::AtMostOnce, true, body)
            .await
            .map_err(|e| IrisError::Mqtt(e.to_string()))?;
        Ok(())
    }
}
