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
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            weather_city: default_weather_city(),
            weather_lat: default_weather_lat(),
            weather_lon: default_weather_lon(),
            ical_url: String::new(),
        }
    }
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
    pub settings: IrisSettings,
    pub mqtt_state: MqttConnectionState,
    pub mqtt_status_detail: String,
    pub reload_counter: u64,
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
