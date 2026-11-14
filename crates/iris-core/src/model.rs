use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScreenType {
    Grafana,
    OpenObserve,
    Web,
    OrgOverview,
    OrgNotes,
    OrgClock,
    OrgAgenda,
    OrgStreamDeck,
    OrgLifeInfra,
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
pub struct OrgFeedData {
    pub weather: Option<WeatherForecast>,
    pub mails: Vec<OrgMail>,
    pub events: Vec<OrgEvent>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfraStatus {
    pub cluster: Option<ClusterStatus>,
    pub open_prs: Vec<GitHubPrSummary>,
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
    pub settings: IrisSettings,
    pub active_screen_id: Option<String>,
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
