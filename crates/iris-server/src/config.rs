use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub state_file: PathBuf,
    pub mqtt_host_override: Option<String>,
    pub mqtt_port_override: Option<u16>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let port = env::var("IRIS_PORT").unwrap_or_else(|_| "8080".to_string());
        let host = env::var("IRIS_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let state_file = env::var("IRIS_STATE_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/iris_state.json"));
        let mqtt_host_override = env::var("IRIS_MQTT_HOST").ok().filter(|s| !s.is_empty());
        let mqtt_port_override = env::var("IRIS_MQTT_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok());

        Self {
            listen_addr: format!("{host}:{port}"),
            state_file,
            mqtt_host_override,
            mqtt_port_override,
        }
    }
}
