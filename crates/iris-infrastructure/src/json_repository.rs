use std::path::PathBuf;

use async_trait::async_trait;
use chrono::Utc;
use iris_application::StateRepository;
use iris_core::{
    DisplayScreen, IrisError, IrisPersistedState, IrisSettings, OrgNote, Result, ScreenType,
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
            settings: IrisSettings::default(),
            active_screen_id: Some("org-overview".to_string()),
        }
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
                url: "https://grafana.zacharie.org/d/".to_string(),
                kiosk_mode: true,
                auto_refresh_seconds: 30,
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
