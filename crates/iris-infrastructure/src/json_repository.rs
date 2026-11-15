use std::path::PathBuf;

use async_trait::async_trait;
use chrono::Utc;
use iris_application::StateRepository;
use iris_core::{
    CommsStatus, DeckActionKind, DeckButton, DisplayScreen, IrisError, IrisPersistedState,
    IrisSettings, OrgNote, Result, ScreenType,
};
use tracing::warn;

pub struct JsonFileRepository {
    path: PathBuf,
}

impl JsonFileRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_state() -> IrisPersistedState {
        IrisPersistedState {
            screens: Self::default_screens(),
            notes: Self::default_notes(),
            mails: vec![],
            events: vec![],
            deck_buttons: Self::default_deck_buttons(),
            comms: CommsStatus::default(),
            shopping_list: vec![],
            drive_orders: vec![],
            agent_reports: vec![],
            settings: IrisSettings::default(),
            active_screen_id: Some("org-overview".to_string()),
        }
    }

    pub fn default_deck_buttons() -> Vec<DeckButton> {
        vec![
            DeckButton {
                id: "deck-teams".to_string(),
                label: "Teams".to_string(),
                subtitle: "Rejoindre / Ouvrir".to_string(),
                icon: "📞".to_string(),
                color: "#6366f1".to_string(),
                category: "Communication".to_string(),
                kind: DeckActionKind::JoinTeams,
                payload: "msteams://".to_string(),
            },
            DeckButton {
                id: "deck-zoom".to_string(),
                label: "Zoom".to_string(),
                subtitle: "Réunion en cours".to_string(),
                icon: "🎥".to_string(),
                color: "#0ea5e9".to_string(),
                category: "Communication".to_string(),
                kind: DeckActionKind::JoinZoom,
                payload: "zoommtg://".to_string(),
            },
            DeckButton {
                id: "deck-openobserve".to_string(),
                label: "OpenObserve".to_string(),
                subtitle: "Logs & Traces".to_string(),
                icon: "🔭".to_string(),
                color: "#06b6d4".to_string(),
                category: "Observabilité".to_string(),
                kind: DeckActionKind::OpenUrl,
                payload: "https://o2-openobserve.p.zacharie.org".to_string(),
            },
            DeckButton {
                id: "deck-grafana".to_string(),
                label: "Grafana".to_string(),
                subtitle: "Cluster jo3".to_string(),
                icon: "📊".to_string(),
                color: "#f97316".to_string(),
                category: "Observabilité".to_string(),
                kind: DeckActionKind::OpenUrl,
                payload: "https://grafana.zacharie.org".to_string(),
            },
            DeckButton {
                id: "deck-code".to_string(),
                label: "VS Code".to_string(),
                subtitle: "Workspace PC".to_string(),
                icon: "💻".to_string(),
                color: "#38bdf8".to_string(),
                category: "Productivité".to_string(),
                kind: DeckActionKind::LaunchApp,
                payload: "code".to_string(),
            },
            DeckButton {
                id: "deck-term".to_string(),
                label: "Terminal".to_string(),
                subtitle: "Windows Terminal".to_string(),
                icon: "⌨️".to_string(),
                color: "#22c55e".to_string(),
                category: "Productivité".to_string(),
                kind: DeckActionKind::LaunchApp,
                payload: "wt".to_string(),
            },
            DeckButton {
                id: "deck-media".to_string(),
                label: "Média".to_string(),
                subtitle: "Play / Pause".to_string(),
                icon: "⏯️".to_string(),
                color: "#a855f7".to_string(),
                category: "Système".to_string(),
                kind: DeckActionKind::MediaToggle,
                payload: String::new(),
            },
            DeckButton {
                id: "deck-lock".to_string(),
                label: "Verrouiller".to_string(),
                subtitle: "Session PC".to_string(),
                icon: "🔒".to_string(),
                color: "#ef4444".to_string(),
                category: "Système".to_string(),
                kind: DeckActionKind::LockScreen,
                payload: String::new(),
            },
            DeckButton {
                id: "deck-uniget".to_string(),
                label: "Update UniGetUI".to_string(),
                subtitle: "Winget & Packages PC".to_string(),
                icon: "📦".to_string(),
                color: "#10b981".to_string(),
                category: "Maintenance".to_string(),
                kind: DeckActionKind::UpdateUniGet,
                payload: "winget upgrade --all --include-unknown --accept-package-agreements --accept-source-agreements".to_string(),
            },
            DeckButton {
                id: "deck-proxmox-update".to_string(),
                label: "Update VMs Proxmox".to_string(),
                subtitle: "APT Nodes & VMs".to_string(),
                icon: "🔄".to_string(),
                color: "#f59e0b".to_string(),
                category: "Maintenance".to_string(),
                kind: DeckActionKind::UpdateProxmoxVms,
                payload: "all".to_string(),
            },
        ]
    }

    pub fn default_screens() -> Vec<DisplayScreen> {
        vec![
            DisplayScreen {
                id: "org-overview".to_string(),
                name: "Hub Organisation".to_string(),
                screen_type: ScreenType::OrgOverview,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-home".to_string(),
                name: "Maison & Énergie".to_string(),
                screen_type: ScreenType::OrgHome,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-life-infra".to_string(),
                name: "Vie & Infra".to_string(),
                screen_type: ScreenType::OrgLifeInfra,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-news".to_string(),
                name: "Actualités".to_string(),
                screen_type: ScreenType::OrgNews,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-agents".to_string(),
                name: "Agents & Upgrades".to_string(),
                screen_type: ScreenType::OrgAgents,
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
            DisplayScreen {
                id: "grafana-alerts".to_string(),
                name: "Grafana Cluster".to_string(),
                screen_type: ScreenType::Grafana,
                url: "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 60,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-stream-deck".to_string(),
                name: "Stream Deck".to_string(),
                screen_type: ScreenType::OrgStreamDeck,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-agenda".to_string(),
                name: "Planning & Mails".to_string(),
                screen_type: ScreenType::OrgAgenda,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-notes".to_string(),
                name: "Notes & Tâches".to_string(),
                screen_type: ScreenType::OrgNotes,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: true,
                is_alert_target: false,
            },
            DisplayScreen {
                id: "org-clock".to_string(),
                name: "Horloge Focus".to_string(),
                screen_type: ScreenType::OrgClock,
                url: String::new(),
                kiosk_mode: true,
                auto_refresh_seconds: 0,
                enabled: false,
                is_alert_target: false,
            },
        ]
    }

    pub fn default_notes() -> Vec<OrgNote> {
        let now = Utc::now().timestamp_millis();
        vec![
            OrgNote {
                id: "note-welcome-1".to_string(),
                text: "Configurer le broker MQTT dans Réglages pour piloter la tablette à distance"
                    .to_string(),
                done: false,
                pinned: true,
                category: "Système".to_string(),
                updated_at_epoch_ms: now,
            },
            OrgNote {
                id: "note-welcome-2".to_string(),
                text: "Vérifier les URLs des dashboards OpenObserve et Grafana".to_string(),
                done: false,
                pinned: false,
                category: "Observabilité".to_string(),
                updated_at_epoch_ms: now,
            },
        ]
    }
}

#[async_trait]
impl StateRepository for JsonFileRepository {
    async fn load(&self) -> Result<IrisPersistedState> {
        if !tokio::fs::try_exists(&self.path).await.unwrap_or(false) {
            return Ok(Self::default_state());
        }

        let raw = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| IrisError::Persistence(e.to_string()))?;

        match serde_json::from_str::<IrisPersistedState>(&raw) {
            Ok(mut state) => {
                if state.screens.is_empty() {
                    state.screens = Self::default_screens();
                } else if !state
                    .screens
                    .iter()
                    .any(|s| s.screen_type == ScreenType::OrgHome)
                {
                    state.screens.insert(
                        1.min(state.screens.len()),
                        DisplayScreen {
                            id: "org-home".to_string(),
                            name: "Maison & Énergie".to_string(),
                            screen_type: ScreenType::OrgHome,
                            url: String::new(),
                            kiosk_mode: true,
                            auto_refresh_seconds: 0,
                            enabled: true,
                            is_alert_target: false,
                        },
                    );
                }
                if !state
                    .screens
                    .iter()
                    .any(|s| s.screen_type == ScreenType::OrgNews)
                {
                    state.screens.insert(
                        2.min(state.screens.len()),
                        DisplayScreen {
                            id: "org-news".to_string(),
                            name: "Actualités".to_string(),
                            screen_type: ScreenType::OrgNews,
                            url: String::new(),
                            kiosk_mode: true,
                            auto_refresh_seconds: 0,
                            enabled: true,
                            is_alert_target: false,
                        },
                    );
                }
                if !state
                    .screens
                    .iter()
                    .any(|s| s.screen_type == ScreenType::OrgAgents)
                {
                    state.screens.insert(
                        3.min(state.screens.len()),
                        DisplayScreen {
                            id: "org-agents".to_string(),
                            name: "Agents & Upgrades".to_string(),
                            screen_type: ScreenType::OrgAgents,
                            url: String::new(),
                            kiosk_mode: true,
                            auto_refresh_seconds: 0,
                            enabled: true,
                            is_alert_target: false,
                        },
                    );
                }
                for screen in &mut state.screens {
                    if screen.screen_type == ScreenType::Grafana
                        && (screen.url.trim().is_empty()
                            || screen.url.trim().ends_with("/d/")
                            || screen.url.trim().ends_with("/d"))
                    {
                        screen.url = "https://grafana.p.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard".to_string();
                    }
                }
                if state.deck_buttons.is_empty() {
                    state.deck_buttons = Self::default_deck_buttons();
                } else {
                    for default_btn in Self::default_deck_buttons() {
                        if !state.deck_buttons.iter().any(|b| b.id == default_btn.id) {
                            state.deck_buttons.push(default_btn);
                        }
                    }
                }
                Ok(state)
            }
            Err(err) => {
                warn!(%err, "Invalid iris_state.json, falling back to default state");
                Ok(Self::default_state())
            }
        }
    }

    async fn save(&self, state: &IrisPersistedState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| IrisError::Persistence(e.to_string()))?;
            }
        }

        let json = serde_json::to_string_pretty(state)
            .map_err(|e| IrisError::Persistence(e.to_string()))?;

        let tmp_path = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp_path, json)
            .await
            .map_err(|e| IrisError::Persistence(e.to_string()))?;
        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .map_err(|e| IrisError::Persistence(e.to_string()))?;
        Ok(())
    }
}
