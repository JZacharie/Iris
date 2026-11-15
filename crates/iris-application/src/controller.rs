use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use iris_core::{
    ActiveAlert, BarthesState, ClusterStatus, CommsStatus, DashboardUrlFormatter, DeckActionEvent,
    DeckActionKind, DeckButton, DisplayScreen, DriveOrder, EcoleDirecteStatus, GitHubPrSummary,
    HomeAssistantData, InfraStatus, IrisCommand, IrisPersistedState, IrisSettings, IrisSnapshot,
    KusanagiInfraData, MqttCommandParser, MqttConnectionState, OrgEvent, OrgFeedData, OrgMail,
    OrgNote, PcTelemetry, Result, ScreenType, ShoppingItem, SlackMessageItem, WeatherForecast,
};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

use crate::ports::{MqttPublisher, StateRepository};

struct InnerState {
    screens: Vec<DisplayScreen>,
    active_screen: Option<DisplayScreen>,
    transient_screen: Option<DisplayScreen>,
    active_alert: Option<ActiveAlert>,
    previous_screen_id_before_alert: Option<String>,
    notes: Vec<OrgNote>,
    weather: Option<WeatherForecast>,
    mails: Vec<OrgMail>,
    events: Vec<OrgEvent>,
    comms: CommsStatus,
    shopping_list: Vec<ShoppingItem>,
    drive_orders: Vec<DriveOrder>,
    infra: InfraStatus,
    home_assistant: Option<HomeAssistantData>,
    ecoledirecte: Option<EcoleDirecteStatus>,
    deck_buttons: Vec<DeckButton>,
    pc_telemetry: Option<PcTelemetry>,
    last_deck_action: Option<DeckActionEvent>,
    settings: IrisSettings,
    mqtt_state: MqttConnectionState,
    mqtt_status_detail: String,
    reload_counter: u64,
}

impl InnerState {
    fn from_persisted(persisted: IrisPersistedState) -> Self {
        let active_screen = persisted
            .active_screen_id
            .as_deref()
            .and_then(|id| {
                persisted
                    .screens
                    .iter()
                    .find(|s| s.id == id && s.enabled)
                    .cloned()
            })
            .or_else(|| persisted.screens.iter().find(|s| s.enabled).cloned())
            .or_else(|| persisted.screens.first().cloned());

        Self {
            screens: persisted.screens,
            active_screen,
            transient_screen: None,
            active_alert: None,
            previous_screen_id_before_alert: None,
            notes: persisted.notes,
            weather: None,
            mails: persisted.mails,
            events: persisted.events,
            comms: persisted.comms,
            shopping_list: persisted.shopping_list,
            drive_orders: persisted.drive_orders,
            infra: InfraStatus::default(),
            home_assistant: None,
            ecoledirecte: None,
            deck_buttons: persisted.deck_buttons,
            pc_telemetry: None,
            last_deck_action: None,
            settings: persisted.settings,
            mqtt_state: MqttConnectionState::Disconnected,
            mqtt_status_detail: String::new(),
            reload_counter: 0,
        }
    }

    fn to_persisted(&self) -> IrisPersistedState {
        IrisPersistedState {
            screens: self.screens.clone(),
            notes: self.notes.clone(),
            mails: self.mails.clone(),
            events: self
                .events
                .iter()
                .filter(|e| e.source != "ical" && e.source != "ecoledirecte")
                .cloned()
                .collect(),
            deck_buttons: self.deck_buttons.clone(),
            comms: self.comms.clone(),
            shopping_list: self.shopping_list.clone(),
            drive_orders: self.drive_orders.clone(),
            settings: self.settings.clone(),
            active_screen_id: self.active_screen.as_ref().map(|s| s.id.clone()),
        }
    }

    fn snapshot(&self) -> IrisSnapshot {
        let displayed = self
            .transient_screen
            .clone()
            .or_else(|| self.active_screen.clone());
        let formatted_url = displayed
            .as_ref()
            .map(DashboardUrlFormatter::format_screen)
            .unwrap_or_default();

        let barthes_state = compute_barthes_state(
            self.active_alert.as_ref(),
            self.pc_telemetry.as_ref(),
            self.infra.cluster.as_ref(),
            self.infra.kusanagi.as_ref(),
            displayed.as_ref(),
        );

        let unread_from_list = self.mails.iter().filter(|m| m.unread).count() as u32;
        let mut comms = self.comms.clone();
        if comms.mails_unread == 0 && unread_from_list > 0 {
            comms.mails_unread = unread_from_list;
        }

        IrisSnapshot {
            screens: self.screens.clone(),
            active_screen: self.active_screen.clone(),
            transient_screen: self.transient_screen.clone(),
            displayed_screen: displayed,
            formatted_url,
            active_alert: self.active_alert.clone(),
            notes: self.notes.clone(),
            org: OrgFeedData {
                weather: self.weather.clone(),
                mails: self.mails.clone(),
                events: self.events.clone(),
                ecoledirecte: self.ecoledirecte.clone(),
            },
            comms,
            shopping_list: self.shopping_list.clone(),
            drive_orders: self.drive_orders.clone(),
            infra: self.infra.clone(),
            home_assistant: self.home_assistant.clone(),
            deck_buttons: self.deck_buttons.clone(),
            pc_telemetry: self.pc_telemetry.clone(),
            last_deck_action: self.last_deck_action.clone(),
            barthes_state,
            settings: self.settings.clone(),
            mqtt_state: self.mqtt_state,
            mqtt_status_detail: self.mqtt_status_detail.clone(),
            reload_counter: self.reload_counter,
        }
    }
}

fn compute_barthes_state(
    alert: Option<&ActiveAlert>,
    pc: Option<&PcTelemetry>,
    cluster: Option<&ClusterStatus>,
    kusanagi: Option<&KusanagiInfraData>,
    displayed: Option<&DisplayScreen>,
) -> BarthesState {
    if alert.is_some_and(|a| !a.acknowledged)
        || cluster.is_some_and(|c| !c.offline_nodes.is_empty())
    {
        return BarthesState::Punctum;
    }
    if let Some(telemetry) = pc {
        if telemetry.active_meeting.is_some() {
            return BarthesState::Presence;
        }
        let low_quota = telemetry.antigravity_tokens_limit > 0
            && telemetry.antigravity_tokens_used * 100 / telemetry.antigravity_tokens_limit >= 85;
        if telemetry.cpu_percent >= 85 || telemetry.ram_percent >= 85 || low_quota {
            return BarthesState::Connotation;
        }
    }
    if let Some(k) = kusanagi {
        let has_crit_alerts = k.alerts.as_ref().is_some_and(|a| a.critical_count > 0);
        let has_degraded_apps = k.argocd.as_ref().is_some_and(|a| a.degraded > 0);
        if has_crit_alerts || has_degraded_apps {
            return BarthesState::Connotation;
        }
    }
    if displayed.is_some_and(|s| {
        matches!(
            s.screen_type,
            ScreenType::OrgOverview
                | ScreenType::OrgAgenda
                | ScreenType::OrgNotes
                | ScreenType::OrgStreamDeck
                | ScreenType::OrgLifeInfra
                | ScreenType::OrgHome
                | ScreenType::OrgNews
        )
    }) {
        return BarthesState::Studium;
    }
    BarthesState::DegreZero
}

/// Deep module orchestrating playlist rotation, incident/alert overrides,
/// MQTT commands, organization notes, and real-time WebSocket broadcasts.
pub struct IrisController {
    inner: RwLock<InnerState>,
    repo: Arc<dyn StateRepository>,
    mqtt_publisher: RwLock<Option<Arc<dyn MqttPublisher>>>,
    updates_tx: broadcast::Sender<IrisSnapshot>,
    deck_actions_tx: broadcast::Sender<DeckActionEvent>,
}

impl IrisController {
    pub fn new(initial: IrisPersistedState, repo: Arc<dyn StateRepository>) -> Arc<Self> {
        let (updates_tx, _) = broadcast::channel(64);
        let (deck_actions_tx, _) = broadcast::channel(64);
        Arc::new(Self {
            inner: RwLock::new(InnerState::from_persisted(initial)),
            repo,
            mqtt_publisher: RwLock::new(None),
            updates_tx,
            deck_actions_tx,
        })
    }

    pub fn subscribe_deck_actions(&self) -> broadcast::Receiver<DeckActionEvent> {
        self.deck_actions_tx.subscribe()
    }

    pub async fn update_pc_telemetry(&self, telemetry: PcTelemetry) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.pc_telemetry = Some(telemetry);
            state.snapshot()
        };
        let _ = self.updates_tx.send(snapshot.clone());
        snapshot
    }

    pub async fn trigger_deck_button(&self, button_id: &str) -> Result<IrisSnapshot> {
        let button = {
            let state = self.inner.read().await;
            state
                .deck_buttons
                .iter()
                .find(|b| b.id == button_id)
                .cloned()
        };
        let Some(btn) = button else {
            return Err(iris_core::IrisError::ScreenNotFound(button_id.to_string()));
        };

        if btn.kind == DeckActionKind::SwitchIrisScreen {
            return self.select_screen(&btn.payload).await;
        }

        let event = DeckActionEvent {
            id: Uuid::new_v4().to_string(),
            button_id: btn.id.clone(),
            label: btn.label.clone(),
            kind: btn.kind,
            payload: btn.payload.clone(),
            requested_at_epoch_ms: Utc::now().timestamp_millis(),
        };

        let snapshot = {
            let mut state = self.inner.write().await;
            state.last_deck_action = Some(event.clone());
            state.snapshot()
        };

        let _ = self.deck_actions_tx.send(event);
        self.broadcast_only(&snapshot).await;
        Ok(snapshot)
    }

    pub async fn upsert_deck_button(&self, button: DeckButton) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(slot) = state.deck_buttons.iter_mut().find(|b| b.id == button.id) {
                *slot = button;
            } else {
                state.deck_buttons.push(button);
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_deck_button(&self, button_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.deck_buttons.retain(|b| b.id != button_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn set_mqtt_publisher(&self, publisher: Arc<dyn MqttPublisher>) {
        let mut guard = self.mqtt_publisher.write().await;
        *guard = Some(publisher);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<IrisSnapshot> {
        self.updates_tx.subscribe()
    }

    pub async fn snapshot(&self) -> IrisSnapshot {
        self.inner.read().await.snapshot()
    }

    pub fn spawn_rotation_loop(self: &Arc<Self>) {
        let controller = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let interval = {
                    let state = controller.inner.read().await;
                    state.settings.rotation_interval_seconds.max(5)
                };
                tokio::time::sleep(Duration::from_secs(u64::from(interval))).await;

                let should_advance = {
                    let state = controller.inner.read().await;
                    let alert_locks_screen =
                        state.active_alert.as_ref().is_some_and(|a| !a.acknowledged);
                    let enabled_count = state.screens.iter().filter(|s| s.enabled).count();
                    state.settings.rotation_enabled
                        && !alert_locks_screen
                        && state.transient_screen.is_none()
                        && enabled_count > 1
                };

                if should_advance {
                    let _ = controller.next_screen().await;
                }
            }
        });
    }

    pub async fn select_screen(&self, target: &str) -> Result<IrisSnapshot> {
        let query = target.trim();
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            let found = state
                .screens
                .iter()
                .find(|s| s.id.eq_ignore_ascii_case(query))
                .or_else(|| {
                    state
                        .screens
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(query))
                })
                .or_else(|| {
                    query
                        .parse::<usize>()
                        .ok()
                        .and_then(|idx| state.screens.iter().filter(|s| s.enabled).nth(idx))
                })
                .cloned();

            let Some(screen) = found else {
                return Err(iris_core::IrisError::ScreenNotFound(query.to_string()));
            };

            state.active_screen = Some(screen);
            state.transient_screen = None;
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        Ok(snapshot)
    }

    pub async fn next_screen(&self) -> Result<IrisSnapshot> {
        self.step_screen(1).await
    }

    pub async fn previous_screen(&self) -> Result<IrisSnapshot> {
        self.step_screen(-1).await
    }

    async fn step_screen(&self, delta: isize) -> Result<IrisSnapshot> {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            let enabled: Vec<DisplayScreen> = state
                .screens
                .iter()
                .filter(|s| s.enabled)
                .cloned()
                .collect();
            if enabled.is_empty() {
                return Ok(state.snapshot());
            }
            let current_idx = state
                .active_screen
                .as_ref()
                .and_then(|cur| enabled.iter().position(|s| s.id == cur.id));
            let len = enabled.len() as isize;
            let next_idx = match current_idx {
                None => 0,
                Some(idx) => ((idx as isize + delta).rem_euclid(len)) as usize,
            };
            state.active_screen = enabled.get(next_idx).cloned();
            state.transient_screen = None;
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        Ok(snapshot)
    }

    pub async fn trigger_alert(&self, alert: ActiveAlert) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            if state.active_alert.is_none() {
                state.previous_screen_id_before_alert =
                    state.active_screen.as_ref().map(|s| s.id.clone());
            }

            let explicit_screen = alert.screen_id.as_deref().and_then(|id| {
                state
                    .screens
                    .iter()
                    .find(|s| s.id.eq_ignore_ascii_case(id))
                    .cloned()
            });

            let transient_alert_screen = if explicit_screen.is_none() {
                alert
                    .dashboard_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .map(|url| DisplayScreen {
                        id: "alert-transient".to_string(),
                        name: format!("Incident : {}", alert.title),
                        screen_type: MqttCommandParser::infer_provider(url),
                        url: url.to_string(),
                        kiosk_mode: true,
                        auto_refresh_seconds: 15,
                        enabled: true,
                        is_alert_target: false,
                    })
            } else {
                None
            };

            let fallback_alert_screen = explicit_screen
                .clone()
                .or_else(|| state.screens.iter().find(|s| s.is_alert_target).cloned())
                .or_else(|| {
                    state
                        .screens
                        .iter()
                        .find(|s| s.screen_type.is_web_based())
                        .cloned()
                });

            info!(
                alert_id = %alert.id,
                title = %alert.title,
                source = %alert.source,
                "Incident triggered — forcing alert dashboard"
            );

            state.active_alert = Some(alert);
            if let Some(target) = fallback_alert_screen {
                state.active_screen = Some(target);
            }
            state.transient_screen = transient_alert_screen;
            state.snapshot()
        };

        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn acknowledge_alert(&self) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            if let Some(alert) = state.active_alert.as_mut() {
                alert.acknowledged = true;
            }
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn clear_alert(&self) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            let restore_id = state.previous_screen_id_before_alert.take();
            if let Some(id) = restore_id {
                if let Some(restored) = state.screens.iter().find(|s| s.id == id).cloned() {
                    state.active_screen = Some(restored);
                }
            }
            state.active_alert = None;
            state.transient_screen = None;
            info!("Incident cleared — restored previous screen");
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn apply_command(&self, command: IrisCommand) -> IrisSnapshot {
        match command {
            IrisCommand::SwitchScreen { target } => {
                if let Err(err) = self.select_screen(&target).await {
                    warn!(%err, "SwitchScreen command ignored");
                }
                self.snapshot().await
            }
            IrisCommand::NextScreen => self.next_screen().await.unwrap_or_else(|_| unreachable!()),
            IrisCommand::PreviousScreen => self
                .previous_screen()
                .await
                .unwrap_or_else(|_| unreachable!()),
            IrisCommand::OpenUrl {
                url,
                title,
                provider,
            } => {
                let snapshot = {
                    let mut state = self.inner.write().await;
                    state.transient_screen = Some(DisplayScreen {
                        id: "mqtt-url".to_string(),
                        name: title,
                        screen_type: provider,
                        url,
                        kiosk_mode: true,
                        auto_refresh_seconds: 0,
                        enabled: true,
                        is_alert_target: false,
                    });
                    state.snapshot()
                };
                self.broadcast_only(&snapshot).await;
                snapshot
            }
            IrisCommand::TriggerAlert(alert) => self.trigger_alert(alert).await,
            IrisCommand::ClearAlert => self.clear_alert().await,
            IrisCommand::SetRotation {
                enabled,
                interval_seconds,
            } => {
                self.update_settings(|s| {
                    s.rotation_enabled = enabled;
                    if let Some(sec) = interval_seconds {
                        s.rotation_interval_seconds = sec.clamp(5, 3600);
                    }
                })
                .await
            }
            IrisCommand::SetBrightness { percent } => {
                self.update_settings(|s| {
                    s.brightness_percent = if percent < 0 {
                        -1
                    } else {
                        percent.clamp(5, 100)
                    };
                })
                .await
            }
            IrisCommand::Reload => self.request_reload().await,
            IrisCommand::UpsertNote {
                id,
                text,
                category,
                done,
                pinned,
            } => self.upsert_note(id, text, category, done, pinned).await,
            IrisCommand::DeleteNote { id } => self.delete_note(&id).await,
            IrisCommand::UpsertMail(mail) => self.upsert_mail(mail).await,
            IrisCommand::UpsertEvent(event) => self.upsert_event(event).await,
            IrisCommand::UpdateComms(comms) => self.update_comms(comms).await,
            IrisCommand::UpsertShoppingItem(item) => self.upsert_shopping_item(item).await,
            IrisCommand::UpsertDriveOrder(order) => self.upsert_drive_order(order).await,
        }
    }

    pub async fn update_comms(&self, comms: CommsStatus) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.comms = comms;
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn set_shopping_list(&self, items: Vec<ShoppingItem>) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.shopping_list = items;
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn upsert_shopping_item(&self, item: ShoppingItem) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(slot) = state.shopping_list.iter_mut().find(|i| i.id == item.id) {
                *slot = item;
            } else {
                state.shopping_list.insert(0, item);
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn toggle_shopping_item(&self, item_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(item) = state.shopping_list.iter_mut().find(|i| i.id == item_id) {
                item.completed = !item.completed;
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_shopping_item(&self, item_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.shopping_list.retain(|i| i.id != item_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn upsert_drive_order(&self, order: DriveOrder) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(slot) = state.drive_orders.iter_mut().find(|o| o.id == order.id) {
                *slot = order;
            } else {
                state.drive_orders.insert(0, order);
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_drive_order(&self, order_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.drive_orders.retain(|o| o.id != order_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn set_cluster_status(&self, cluster: ClusterStatus) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.infra.cluster = Some(cluster);
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_open_prs(&self, prs: Vec<GitHubPrSummary>) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.infra.open_prs = prs;
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_weather(&self, weather: WeatherForecast) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.weather = Some(weather);
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_ical_events(&self, ical_events: Vec<OrgEvent>) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.events.retain(|e| e.source != "ical");
            state.events.extend(ical_events);
            state.events.sort_by(|a, b| {
                a.date
                    .cmp(&b.date)
                    .then_with(|| a.start_time.cmp(&b.start_time))
            });
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_home_assistant(&self, data: HomeAssistantData) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.home_assistant = Some(data);
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_kusanagi_infra(&self, data: KusanagiInfraData) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            if state.infra.cluster.is_none() {
                if let Some(c) = &data.cluster {
                    state.infra.cluster = Some(ClusterStatus {
                        nodes_total: c.nodes_total,
                        nodes_ready: c.nodes_ready,
                        offline_nodes: Vec::new(),
                        checked_at_epoch_ms: data.updated_at_epoch_ms,
                    });
                }
            }
            state.infra.kusanagi = Some(data);
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn set_ecoledirecte_events(
        &self,
        ed_events: Vec<OrgEvent>,
        status: EcoleDirecteStatus,
    ) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.ecoledirecte = Some(status);
            state.events.retain(|e| e.source != "ecoledirecte");
            state.events.extend(ed_events);
            state.events.sort_by(|a, b| {
                a.date
                    .cmp(&b.date)
                    .then_with(|| a.start_time.cmp(&b.start_time))
            });
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn apply_fast_telemetry(
        &self,
        ha: Option<HomeAssistantData>,
        kusanagi: Option<KusanagiInfraData>,
        slack_messages: Option<Vec<SlackMessageItem>>,
    ) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            if let Some(ha_data) = ha {
                state.home_assistant = Some(ha_data);
            }
            if let Some(k_data) = kusanagi {
                if state.infra.cluster.is_none() {
                    if let Some(c) = &k_data.cluster {
                        state.infra.cluster = Some(ClusterStatus {
                            nodes_total: c.nodes_total,
                            nodes_ready: c.nodes_ready,
                            offline_nodes: Vec::new(),
                            checked_at_epoch_ms: k_data.updated_at_epoch_ms,
                        });
                    }
                }
                state.infra.kusanagi = Some(k_data);
            }
            if let Some(msgs) = slack_messages {
                state.comms.slack_unread = msgs.len() as u32;
                state.comms.slack_messages = msgs;
                state.comms.updated_at_epoch_ms = Utc::now().timestamp_millis();
            }
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn apply_slow_feeds(
        &self,
        weather: Option<WeatherForecast>,
        ical_events: Option<Vec<OrgEvent>>,
        cluster: Option<ClusterStatus>,
        open_prs: Option<Vec<GitHubPrSummary>>,
        shopping: Option<Vec<ShoppingItem>>,
        ed: Option<(Vec<OrgEvent>, EcoleDirecteStatus)>,
    ) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            if let Some(w) = weather {
                state.weather = Some(w);
            }
            if let Some(ical) = ical_events {
                state.events.retain(|e| e.source != "ical");
                state.events.extend(ical);
            }
            if let Some(c) = cluster {
                state.infra.cluster = Some(c);
            }
            if let Some(prs) = open_prs {
                state.infra.open_prs = prs;
            }
            if let Some(items) = shopping {
                state.shopping_list = items;
            }
            if let Some((ed_events, ed_status)) = ed {
                state.ecoledirecte = Some(ed_status);
                state.events.retain(|e| e.source != "ecoledirecte");
                state.events.extend(ed_events);
            }
            state.events.sort_by(|a, b| {
                a.date
                    .cmp(&b.date)
                    .then_with(|| a.start_time.cmp(&b.start_time))
            });
            state.snapshot()
        };
        self.broadcast_only(&snapshot).await;
        snapshot
    }

    pub async fn upsert_mail(&self, mail: OrgMail) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(slot) = state.mails.iter_mut().find(|m| m.id == mail.id) {
                *slot = mail;
            } else {
                state.mails.insert(0, mail);
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_mail(&self, mail_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.mails.retain(|m| m.id != mail_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn upsert_event(&self, event: OrgEvent) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(slot) = state.events.iter_mut().find(|e| e.id == event.id) {
                *slot = event;
            } else {
                state.events.push(event);
            }
            state.events.sort_by(|a, b| {
                a.date
                    .cmp(&b.date)
                    .then_with(|| a.start_time.cmp(&b.start_time))
            });
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_event(&self, event_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.events.retain(|e| e.id != event_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn upsert_screen(&self, screen: DisplayScreen) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if screen.is_alert_target {
                for existing in &mut state.screens {
                    existing.is_alert_target = false;
                }
            }
            if let Some(slot) = state.screens.iter_mut().find(|s| s.id == screen.id) {
                *slot = screen.clone();
            } else {
                state.screens.push(screen.clone());
            }
            if state
                .active_screen
                .as_ref()
                .is_some_and(|a| a.id == screen.id)
            {
                state.active_screen = Some(screen);
            } else if state.active_screen.is_none() {
                state.active_screen = state.screens.iter().find(|s| s.enabled).cloned();
            }
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_screen(&self, screen_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.screens.retain(|s| s.id != screen_id);
            if state
                .active_screen
                .as_ref()
                .is_some_and(|a| a.id == screen_id)
            {
                state.active_screen = state
                    .screens
                    .iter()
                    .find(|s| s.enabled)
                    .cloned()
                    .or_else(|| state.screens.first().cloned());
            }
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn upsert_note(
        &self,
        id: Option<String>,
        text: String,
        category: String,
        done: bool,
        pinned: bool,
    ) -> IrisSnapshot {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return self.snapshot().await;
        }
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            let note_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
            let new_note = OrgNote {
                id: note_id.clone(),
                text: trimmed.to_string(),
                done,
                pinned,
                category,
                updated_at_epoch_ms: Utc::now().timestamp_millis(),
            };
            if let Some(slot) = state.notes.iter_mut().find(|n| n.id == note_id) {
                *slot = new_note;
            } else {
                state.notes.insert(0, new_note);
            }
            (state.snapshot(), state.to_persisted())
        };

        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn toggle_note_done(&self, note_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            if let Some(note) = state.notes.iter_mut().find(|n| n.id == note_id) {
                note.done = !note.done;
                note.updated_at_epoch_ms = Utc::now().timestamp_millis();
            }
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn delete_note(&self, note_id: &str) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            state.notes.retain(|n| n.id != note_id);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn update_settings(&self, mut f: impl FnMut(&mut IrisSettings)) -> IrisSnapshot {
        let (snapshot, persisted) = {
            let mut state = self.inner.write().await;
            f(&mut state.settings);
            (state.snapshot(), state.to_persisted())
        };
        self.persist_and_broadcast(&persisted, &snapshot).await;
        snapshot
    }

    pub async fn update_mqtt_status(&self, status: MqttConnectionState, detail: impl Into<String>) {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.mqtt_state = status;
            state.mqtt_status_detail = detail.into();
            state.snapshot()
        };
        let _ = self.updates_tx.send(snapshot);
    }

    pub async fn request_reload(&self) -> IrisSnapshot {
        let snapshot = {
            let mut state = self.inner.write().await;
            state.reload_counter = state.reload_counter.saturating_add(1);
            state.snapshot()
        };
        let _ = self.updates_tx.send(snapshot.clone());
        snapshot
    }

    async fn persist_and_broadcast(&self, persisted: &IrisPersistedState, snapshot: &IrisSnapshot) {
        if let Err(err) = self.repo.save(persisted).await {
            warn!(%err, "Failed to persist Iris state");
        }
        self.broadcast_only(snapshot).await;
    }

    async fn broadcast_only(&self, snapshot: &IrisSnapshot) {
        let _ = self.updates_tx.send(snapshot.clone());
        let publisher = self.mqtt_publisher.read().await.clone();
        if let Some(pub_impl) = publisher {
            if let Err(err) = pub_impl.publish_state(snapshot).await {
                warn!(%err, "Failed to publish Iris state to MQTT");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris_core::{AlertSeverity, ScreenType};

    struct InMemoryRepo;

    #[async_trait::async_trait]
    impl StateRepository for InMemoryRepo {
        async fn load(&self) -> Result<IrisPersistedState> {
            Ok(sample_state())
        }
        async fn save(&self, _state: &IrisPersistedState) -> Result<()> {
            Ok(())
        }
    }

    fn sample_state() -> IrisPersistedState {
        IrisPersistedState {
            screens: vec![
                DisplayScreen {
                    id: "org-overview".to_string(),
                    name: "Hub".to_string(),
                    screen_type: ScreenType::OrgOverview,
                    url: String::new(),
                    kiosk_mode: true,
                    auto_refresh_seconds: 0,
                    enabled: true,
                    is_alert_target: false,
                },
                DisplayScreen {
                    id: "o2-overview".to_string(),
                    name: "OpenObserve".to_string(),
                    screen_type: ScreenType::OpenObserve,
                    url: "https://o2-openobserve.p.zacharie.org/web/dashboards".to_string(),
                    kiosk_mode: true,
                    auto_refresh_seconds: 60,
                    enabled: true,
                    is_alert_target: true,
                },
            ],
            notes: vec![],
            mails: vec![],
            events: vec![],
            deck_buttons: vec![],
            comms: CommsStatus::default(),
            shopping_list: vec![],
            drive_orders: vec![],
            settings: IrisSettings::default(),
            active_screen_id: Some("org-overview".to_string()),
        }
    }

    #[tokio::test]
    async fn alert_forces_target_and_restores_previous_screen() {
        let controller = IrisController::new(sample_state(), Arc::new(InMemoryRepo));
        assert_eq!(
            controller
                .snapshot()
                .await
                .displayed_screen
                .as_ref()
                .map(|s| s.id.as_str()),
            Some("org-overview")
        );

        let after_alert = controller
            .trigger_alert(ActiveAlert {
                id: "a1".to_string(),
                severity: AlertSeverity::Critical,
                title: "High CPU".to_string(),
                message: "Node 1".to_string(),
                source: "openobserve".to_string(),
                dashboard_url: None,
                screen_id: None,
                triggered_at_epoch_ms: 0,
                acknowledged: false,
            })
            .await;

        assert_eq!(
            after_alert.displayed_screen.as_ref().map(|s| s.id.as_str()),
            Some("o2-overview")
        );

        let after_clear = controller.clear_alert().await;
        assert!(after_clear.active_alert.is_none());
        assert_eq!(
            after_clear.displayed_screen.as_ref().map(|s| s.id.as_str()),
            Some("org-overview")
        );
    }

    #[tokio::test]
    async fn mqtt_note_command_adds_note() {
        let controller = IrisController::new(sample_state(), Arc::new(InMemoryRepo));
        let snap = controller
            .apply_command(IrisCommand::UpsertNote {
                id: None,
                text: "Vérifier backup MinIO".to_string(),
                category: "Infra".to_string(),
                done: false,
                pinned: true,
            })
            .await;

        assert_eq!(snap.notes.len(), 1);
        assert_eq!(snap.notes[0].text, "Vérifier backup MinIO");
    }

    #[tokio::test]
    async fn kusanagi_critical_alerts_shift_barthes_to_connotation() {
        use iris_core::KusanagiAlertsSummary;
        let controller = IrisController::new(sample_state(), Arc::new(InMemoryRepo));
        let kusanagi = KusanagiInfraData {
            connected: true,
            updated_at_epoch_ms: 1000,
            alerts: Some(KusanagiAlertsSummary {
                critical_count: 2,
                warning_count: 0,
                top_alerts: Vec::new(),
            }),
            ..Default::default()
        };
        let snap = controller
            .apply_fast_telemetry(None, Some(kusanagi), None)
            .await;
        assert_eq!(snap.barthes_state, BarthesState::Connotation);
    }
}
