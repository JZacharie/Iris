use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScreenType {
    Grafana,
    #[serde(alias = "OPENOBSERVE")]
    OpenObserve,
    Web,
    OrgOverview,
    OrgNotes,
    OrgClock,
    OrgAgenda,
    OrgStreamDeck,
    OrgLifeInfra,
    OrgHome,
    OrgNews,
    OrgAgents,
    #[serde(alias = "ORG_3D_PRINT")]
    Org3DPrint,
}

impl ScreenType {
    pub fn is_web_based(self) -> bool {
        matches!(self, Self::Grafana | Self::OpenObserve | Self::Web)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Grafana => "Grafana",
            Self::OpenObserve => "OpenObserve",
            Self::Web => "Web / URL",
            Self::OrgOverview => "Organisation — Hub",
            Self::OrgNotes => "Organisation — Notes",
            Self::OrgClock => "Organisation — Horloge",
            Self::OrgAgenda => "Organisation — Planning & Mails",
            Self::OrgStreamDeck => "Stream Deck & PC",
            Self::OrgLifeInfra => "Vie Quotidienne & Infra",
            Self::OrgHome => "Maison & Énergie",
            Self::OrgNews => "Actualités Kusanagi",
            Self::OrgAgents => "Rapports Agents & Upgrades",
            Self::Org3DPrint => "Impression 3D & Ender",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayScreen {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub screen_type: ScreenType,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_true")]
    pub kiosk_mode: bool,
    #[serde(default)]
    pub auto_refresh_seconds: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_alert_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl AlertSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "CRITIQUE",
            Self::Warning => "ALERTE",
            Self::Info => "INFO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAlert {
    pub id: String,
    pub severity: AlertSeverity,
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default = "default_mqtt_source")]
    pub source: String,
    #[serde(default)]
    pub dashboard_url: Option<String>,
    #[serde(default)]
    pub screen_id: Option<String>,
    pub triggered_at_epoch_ms: i64,
    #[serde(default)]
    pub acknowledged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgNote {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "default_category")]
    pub category: String,
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_broker_host")]
    pub broker_host: String,
    #[serde(default = "default_broker_port")]
    pub broker_port: u16,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_device_id")]
    pub device_id: String,
    #[serde(default = "default_base_topic")]
    pub base_topic: String,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            broker_host: default_broker_host(),
            broker_port: default_broker_port(),
            use_tls: false,
            username: String::new(),
            password: String::new(),
            device_id: default_device_id(),
            base_topic: default_base_topic(),
        }
    }
}

impl MqttConfig {
    pub fn normalized_base_topic(&self) -> String {
        let trimmed = self.base_topic.trim().trim_matches('/');
        if trimmed.is_empty() {
            "iris".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn normalized_device_id(&self) -> String {
        let trimmed = self.device_id.trim().trim_matches('/');
        if trimmed.is_empty() {
            "tablet-1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn device_topic_prefix(&self) -> String {
        format!(
            "{}/{}",
            self.normalized_base_topic(),
            self.normalized_device_id()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrisSettings {
    #[serde(default)]
    pub mqtt: MqttConfig,
    #[serde(default)]
    pub org: OrgConfig,
    #[serde(default = "default_true")]
    pub rotation_enabled: bool,
    #[serde(default = "default_rotation_interval")]
    pub rotation_interval_seconds: u32,
    #[serde(default = "default_true")]
    pub keep_screen_on: bool,
    #[serde(default = "default_true")]
    pub immersive_fullscreen: bool,
    #[serde(default)]
    pub auto_hide_hud: bool,
    #[serde(default = "default_brightness")]
    pub brightness_percent: i32,
}

impl Default for IrisSettings {
    fn default() -> Self {
        Self {
            mqtt: MqttConfig::default(),
            org: OrgConfig::default(),
            rotation_enabled: true,
            rotation_interval_seconds: default_rotation_interval(),
            keep_screen_on: true,
            immersive_fullscreen: true,
            auto_hide_hud: false,
            brightness_percent: default_brightness(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgConfig {
    #[serde(default = "default_weather_city")]
    pub weather_city: String,
    #[serde(default = "default_weather_lat")]
    pub weather_lat: String,
    #[serde(default = "default_weather_lon")]
    pub weather_lon: String,
    #[serde(default)]
    pub ical_url: String,
    #[serde(default = "default_github_repos")]
    pub github_repos: Vec<String>,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            weather_city: default_weather_city(),
            weather_lat: default_weather_lat(),
            weather_lon: default_weather_lon(),
            ical_url: String::new(),
            github_repos: default_github_repos(),
        }
    }
}

fn default_github_repos() -> Vec<String> {
    vec![
        "JZacharie/jo3".to_string(),
        "JZacharie/Iris".to_string(),
        "JZacharie/Kusanagi".to_string(),
        "JZacharie/Pylos".to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherDay {
    pub date: String,
    pub label: String,
    pub icon: String,
    pub temp_min_c: i32,
    pub temp_max_c: i32,
    pub precip_prob_pct: u32,
    pub wind_max_kmh: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherForecast {
    pub city: String,
    pub current_temp_c: i32,
    pub current_label: String,
    pub current_icon: String,
    pub days: Vec<WeatherDay>,
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMail {
    pub id: String,
    pub sender: String,
    pub subject: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub received_at: String,
    #[serde(default = "default_true")]
    pub unread: bool,
    #[serde(default)]
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgEvent {
    pub id: String,
    pub title: String,
    pub date: String,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub end_time: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoleDirecteStatus {
    pub configured: bool,
    pub connected: bool,
    #[serde(default)]
    pub student_id: String,
    #[serde(default)]
    pub courses_count: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgFeedData {
    pub weather: Option<WeatherForecast>,
    pub mails: Vec<OrgMail>,
    pub events: Vec<OrgEvent>,
    #[serde(default)]
    pub ecoledirecte: Option<EcoleDirecteStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BarthesState {
    /// Équilibre silencieux : système nominal, aucune friction
    DegreZero,
    /// Attention calme : lecture organisationnelle et veille active
    Studium,
    /// Signal faible : charge CPU/RAM élevée ou quota Antigravity bas
    Connotation,
    /// En communication : appel Teams ou Zoom détecté sur le poste
    Presence,
    /// Saillance / Rupture : incident critique qui perce l'écran
    Punctum,
}

impl BarthesState {
    pub fn label(self) -> &'static str {
        match self {
            Self::DegreZero => "Degré zéro",
            Self::Studium => "Studium",
            Self::Connotation => "Connotation",
            Self::Presence => "Présence",
            Self::Punctum => "Punctum",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::DegreZero => "Équilibre silencieux — flux nominal",
            Self::Studium => "Attention calme — veille organisationnelle",
            Self::Connotation => "Signal faible — tension ressources ou quota",
            Self::Presence => "En communication — Teams / Zoom actif",
            Self::Punctum => "Rupture critique — incident actif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeckActionKind {
    JoinTeams,
    JoinZoom,
    OpenUrl,
    LaunchApp,
    MediaToggle,
    LockScreen,
    SwitchIrisScreen,
    UpdateUniGet,
    UpdateProxmoxVms,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckButton {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_deck_color")]
    pub color: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub kind: DeckActionKind,
    #[serde(default)]
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckActionEvent {
    pub id: String,
    pub button_id: String,
    pub label: String,
    pub kind: DeckActionKind,
    pub payload: String,
    pub requested_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PcTelemetry {
    pub hostname: String,
    pub cpu_percent: u32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub ram_percent: u32,
    #[serde(default)]
    pub gpu_name: String,
    #[serde(default)]
    pub gpu_percent: u32,
    #[serde(default)]
    pub gpu_mem_used_mb: u64,
    #[serde(default)]
    pub gpu_mem_total_mb: u64,
    #[serde(default)]
    pub gpu_mem_percent: u32,
    pub uptime_secs: u64,
    #[serde(default)]
    pub active_meeting: Option<String>,
    #[serde(default)]
    pub antigravity_tokens_used: u64,
    #[serde(default = "default_antigravity_limit")]
    pub antigravity_tokens_limit: u64,
    #[serde(default)]
    pub antigravity_model: String,
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlackMessageItem {
    pub id: String,
    pub channel: String,
    pub sender: String,
    pub text: String,
    pub ts_label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommsStatus {
    #[serde(default)]
    pub missed_calls: u32,
    #[serde(default)]
    pub missed_call_details: Vec<String>,
    #[serde(default)]
    pub voicemails: u32,
    #[serde(default)]
    pub voicemail_details: Vec<String>,
    #[serde(default)]
    pub teams_unread: u32,
    #[serde(default)]
    pub mails_unread: u32,
    #[serde(default)]
    pub slack_unread: u32,
    #[serde(default)]
    pub slack_messages: Vec<SlackMessageItem>,
    #[serde(default)]
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShoppingItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveOrder {
    pub id: String,
    #[serde(default = "default_drive_store")]
    pub store: String,
    pub status: String,
    #[serde(default)]
    pub pickup_slot: String,
    #[serde(default)]
    pub ready_for_pickup: bool,
}

fn default_drive_store() -> String {
    "ClearDrive".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatus {
    pub nodes_total: u32,
    pub nodes_ready: u32,
    pub offline_nodes: Vec<String>,
    pub checked_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPrSummary {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub author: String,
    pub url: String,
    pub is_dependency_update: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaPerson {
    pub id: String,
    pub name: String,
    pub state: String,
    pub is_home: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaVehicle {
    pub name: String,
    pub battery_pct: Option<u32>,
    pub range_km: Option<u32>,
    pub charging_status: String,
    pub location: String,
    pub lock_state: String,
    pub odometer_km: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaPowerPlug {
    pub entity_id: String,
    pub name: String,
    pub power_w: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaEnergy {
    pub solar_now_w: Option<i32>,
    #[serde(default)]
    pub home_consumption_w: Option<i32>,
    #[serde(default)]
    pub net_grid_w: Option<i32>,
    pub solar_today_kwh: String,
    pub forecast_today_kwh: String,
    pub forecast_tomorrow_kwh: String,
    pub plugs: Vec<HaPowerPlug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaClimateSensor {
    pub name: String,
    pub temp_c: String,
    pub humidity_pct: Option<u32>,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaNasStatus {
    pub name: String,
    pub cpu_pct: Option<u32>,
    pub ram_pct: Option<u32>,
    pub volume_used_pct: String,
    pub disk_temps_c: Vec<i32>,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaDeviceSummary {
    pub alarm_state: String,
    pub vacuum_state: String,
    pub vacuum_battery_pct: Option<u32>,
    pub covers_open: u32,
    pub covers_total: u32,
    pub open_cover_names: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeAssistantData {
    pub connected: bool,
    pub updated_at_epoch_ms: i64,
    pub people: Vec<HaPerson>,
    pub vehicle: Option<HaVehicle>,
    pub energy: HaEnergy,
    pub climate: Vec<HaClimateSensor>,
    pub nas: Option<HaNasStatus>,
    pub devices: HaDeviceSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiClusterMetrics {
    pub nodes_ready: u32,
    pub nodes_total: u32,
    pub pods_running: u32,
    pub pods_total: u32,
    pub services_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiProxmoxNode {
    pub node: String,
    pub status: String,
    pub cpu_pct: u32,
    pub mem_used_gb: String,
    pub mem_total_gb: String,
    pub mem_pct: u32,
    pub disk_used_gb: String,
    pub disk_total_gb: String,
    pub disk_pct: u32,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiArgoCdSummary {
    pub total: u32,
    pub healthy: u32,
    pub synced: u32,
    pub degraded: u32,
    pub out_of_sync: u32,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiAlertItem {
    pub name: String,
    pub namespace: String,
    pub severity: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiAlertsSummary {
    pub critical_count: u32,
    pub warning_count: u32,
    pub top_alerts: Vec<KusanagiAlertItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiNewsItem {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiProxmoxVm {
    pub vmid: u32,
    pub name: String,
    pub node: String,
    pub kind: String,
    pub status: String,
    pub cpu_pct: u32,
    pub mem_used_gb: String,
    pub mem_total_gb: String,
    pub mem_pct: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KusanagiInfraData {
    pub connected: bool,
    pub updated_at_epoch_ms: i64,
    pub cluster: Option<KusanagiClusterMetrics>,
    pub proxmox_nodes: Vec<KusanagiProxmoxNode>,
    #[serde(default)]
    pub proxmox_vms: Vec<KusanagiProxmoxVm>,
    pub argocd: Option<KusanagiArgoCdSummary>,
    pub alerts: Option<KusanagiAlertsSummary>,
    #[serde(default)]
    pub news: Vec<KusanagiNewsItem>,
    #[serde(default)]
    pub news_sources: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfraStatus {
    pub cluster: Option<ClusterStatus>,
    pub open_prs: Vec<GitHubPrSummary>,
    #[serde(default)]
    pub kusanagi: Option<KusanagiInfraData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MqttConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpgradeReport {
    pub id: String,
    pub agent_name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub status: String,
    #[serde(default = "default_true")]
    pub is_real_success: bool,
    #[serde(default)]
    pub summary_note: String,
    #[serde(default)]
    pub report_output: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrisPersistedState {
    pub screens: Vec<DisplayScreen>,
    pub notes: Vec<OrgNote>,
    #[serde(default)]
    pub mails: Vec<OrgMail>,
    #[serde(default)]
    pub events: Vec<OrgEvent>,
    #[serde(default)]
    pub deck_buttons: Vec<DeckButton>,
    #[serde(default)]
    pub comms: CommsStatus,
    #[serde(default)]
    pub shopping_list: Vec<ShoppingItem>,
    #[serde(default)]
    pub drive_orders: Vec<DriveOrder>,
    #[serde(default)]
    pub agent_reports: Vec<AgentUpgradeReport>,
    pub settings: IrisSettings,
    pub active_screen_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Printer3DStatus {
    pub connected: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub progress_pct: u32,
    #[serde(default)]
    pub time_elapsed_secs: u64,
    #[serde(default)]
    pub time_remaining_secs: u64,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub nozzle_temp_c: i32,
    #[serde(default)]
    pub nozzle_target_c: i32,
    #[serde(default)]
    pub bed_temp_c: i32,
    #[serde(default)]
    pub bed_target_c: i32,
    #[serde(default)]
    pub plug_on: bool,
    #[serde(default)]
    pub power_w: i32,
    #[serde(default)]
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceIntentResult {
    pub raw_text: String,
    pub intent_type: String,
    pub spoken_reply: String,
    #[serde(default)]
    pub target_screen_id: Option<String>,
    #[serde(default)]
    pub action_executed: Option<String>,
    pub timestamp_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrisSnapshot {
    pub screens: Vec<DisplayScreen>,
    pub active_screen: Option<DisplayScreen>,
    pub transient_screen: Option<DisplayScreen>,
    pub displayed_screen: Option<DisplayScreen>,
    pub formatted_url: String,
    pub active_alert: Option<ActiveAlert>,
    pub notes: Vec<OrgNote>,
    pub org: OrgFeedData,
    pub comms: CommsStatus,
    pub shopping_list: Vec<ShoppingItem>,
    pub drive_orders: Vec<DriveOrder>,
    pub infra: InfraStatus,
    #[serde(default)]
    pub home_assistant: Option<HomeAssistantData>,
    #[serde(default)]
    pub agent_reports: Vec<AgentUpgradeReport>,
    #[serde(default)]
    pub printer3d: Option<Printer3DStatus>,
    #[serde(default)]
    pub last_voice_intent: Option<VoiceIntentResult>,
    pub deck_buttons: Vec<DeckButton>,
    pub pc_telemetry: Option<PcTelemetry>,
    pub last_deck_action: Option<DeckActionEvent>,
    pub barthes_state: BarthesState,
    pub settings: IrisSettings,
    pub mqtt_state: MqttConnectionState,
    pub mqtt_status_detail: String,
    pub reload_counter: u64,
}

fn default_deck_color() -> String {
    "#38bdf8".to_string()
}
fn default_antigravity_limit() -> u64 {
    2_000_000
}

fn default_weather_city() -> String {
    "Lyon".to_string()
}
fn default_weather_lat() -> String {
    "45.7640".to_string()
}
fn default_weather_lon() -> String {
    "4.8357".to_string()
}

fn default_true() -> bool {
    true
}
fn default_mqtt_source() -> String {
    "mqtt".to_string()
}
fn default_category() -> String {
    "Organisation".to_string()
}
fn default_broker_host() -> String {
    "192.168.0.170".to_string()
}
fn default_broker_port() -> u16 {
    1883
}
fn default_device_id() -> String {
    "tablet-1".to_string()
}
fn default_base_topic() -> String {
    "iris".to_string()
}
fn default_rotation_interval() -> u32 {
    45
}
fn default_brightness() -> i32 {
    -1
}
