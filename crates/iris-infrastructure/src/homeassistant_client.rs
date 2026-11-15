use std::collections::HashMap;
use std::env;
use std::time::Duration;

use chrono::Utc;
use iris_core::{
    HaClimateSensor, HaDeviceSummary, HaEnergy, HaNasStatus, HaPerson, HaPowerPlug, HaVehicle,
    HomeAssistantData, IrisError, Result,
};
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_HA_URL: &str = "https://vha.zacharie.org";

#[derive(Deserialize)]
struct HaRawState {
    entity_id: String,
    state: String,
    #[serde(default)]
    attributes: HashMap<String, Value>,
}

pub struct HomeAssistantClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl HomeAssistantClient {
    pub fn from_env() -> Self {
        let base_url = env::var("IRIS_HOME_ASSISTANT_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                env::var("HOME_ASSISTANT_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_HA_URL.to_string());

        let token = env::var("IRIS_HOME_ASSISTANT_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                env::var("HOME_ASSISTANT_TOKEN")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_default();

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            http,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.token.trim().is_empty()
    }

    pub async fn fetch_data(&self) -> Result<HomeAssistantData> {
        if !self.is_configured() {
            return Err(IrisError::InvalidConfig(
                "IRIS_HOME_ASSISTANT_TOKEN not configured".to_string(),
            ));
        }

        let url = format!("{}/api/states", self.base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("HA request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "HA API returned status {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("HA body error: {e}")))?;

        parse_ha_states_json(&body, Utc::now().timestamp_millis())
    }
}

impl Default for HomeAssistantClient {
    fn default() -> Self {
        Self::from_env()
    }
}

pub fn parse_ha_states_json(body: &str, updated_at_epoch_ms: i64) -> Result<HomeAssistantData> {
    let raw_states: Vec<HaRawState> = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("invalid HA states JSON: {e}")))?;

    let by_id: HashMap<&str, &HaRawState> = raw_states
        .iter()
        .map(|s| (s.entity_id.as_str(), s))
        .collect();

    let people = extract_people(&raw_states);
    let vehicle = extract_vehicle(&by_id);
    let energy = extract_energy(&by_id, &raw_states);
    let climate = extract_climate(&by_id);
    let nas = extract_nas(&by_id);
    let devices = extract_devices(&by_id, &raw_states);

    Ok(HomeAssistantData {
        connected: true,
        updated_at_epoch_ms,
        people,
        vehicle,
        energy,
        climate,
        nas,
        devices,
    })
}

fn friendly_name(s: &HaRawState) -> String {
    s.attributes
        .get("friendly_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&s.entity_id)
        .to_string()
}

fn valid_state<'a>(by_id: &HashMap<&str, &'a HaRawState>, id: &str) -> Option<&'a HaRawState> {
    by_id
        .get(id)
        .copied()
        .filter(|s| !matches!(s.state.as_str(), "unavailable" | "unknown" | ""))
}

fn parse_u32(s: &str) -> Option<u32> {
    s.trim().parse::<f64>().ok().map(|v| v.round() as u32)
}

fn parse_i32(s: &str) -> Option<i32> {
    s.trim().parse::<f64>().ok().map(|v| v.round() as i32)
}

fn extract_people(states: &[HaRawState]) -> Vec<HaPerson> {
    let mut people: Vec<HaPerson> = states
        .iter()
        .filter(|s| {
            s.entity_id.starts_with("person.")
                && !matches!(s.state.as_str(), "unavailable" | "unknown")
        })
        .map(|s| HaPerson {
            id: s.entity_id.clone(),
            name: friendly_name(s),
            state: s.state.clone(),
            is_home: s.state == "home",
        })
        .collect();
    people.sort_by(|a, b| a.name.cmp(&b.name));
    people
}

fn extract_vehicle(by_id: &HashMap<&str, &HaRawState>) -> Option<HaVehicle> {
    let bat = valid_state(by_id, "sensor.volvo_ex40_batterie").and_then(|s| parse_u32(&s.state));
    let range = valid_state(by_id, "sensor.volvo_ex40_autonomie").and_then(|s| parse_u32(&s.state));
    let charging = valid_state(by_id, "sensor.volvo_ex40_statut_charge")
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "—".to_string());
    let location = valid_state(by_id, "sensor.volvo_ex40_localisation")
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "—".to_string());
    let lock_state = valid_state(by_id, "input_select.volvo_etat_verrouillage")
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "—".to_string());
    let odometer =
        valid_state(by_id, "sensor.volvo_ex40_odometre").and_then(|s| parse_u32(&s.state));

    if bat.is_none() && range.is_none() {
        return None;
    }

    Some(HaVehicle {
        name: "Volvo EX40".to_string(),
        battery_pct: bat,
        range_km: range,
        charging_status: charging,
        location,
        lock_state,
        odometer_km: odometer,
    })
}

fn extract_energy(by_id: &HashMap<&str, &HaRawState>, states: &[HaRawState]) -> HaEnergy {
    let solar_now_w = valid_state(by_id, "sensor.envoy_122304017410_current_power_production")
        .and_then(|s| {
            let val = s.state.trim().parse::<f64>().ok()?;
            let unit = s
                .attributes
                .get("unit_of_measurement")
                .and_then(|v| v.as_str())
                .unwrap_or("W");
            if unit.eq_ignore_ascii_case("kW") {
                Some((val * 1000.0).round() as i32)
            } else {
                Some(val.round() as i32)
            }
        })
        .or_else(|| {
            valid_state(by_id, "sensor.power_production_now").and_then(|s| parse_i32(&s.state))
        });

    let solar_today_kwh = valid_state(by_id, "sensor.envoy_122304017410_today_s_energy_production")
        .map(|s| format_decimal(&s.state, 1))
        .unwrap_or_else(|| "—".to_string());

    let forecast_today_kwh = valid_state(by_id, "sensor.energy_production_today")
        .map(|s| format_decimal(&s.state, 1))
        .unwrap_or_else(|| "—".to_string());

    let forecast_tomorrow_kwh = valid_state(by_id, "sensor.energy_production_tomorrow")
        .map(|s| format_decimal(&s.state, 1))
        .unwrap_or_else(|| "—".to_string());

    let mut plugs: Vec<HaPowerPlug> = states
        .iter()
        .filter(|s| {
            s.entity_id.starts_with("sensor.")
                && !matches!(s.state.as_str(), "unavailable" | "unknown")
                && s.attributes.get("device_class").and_then(|v| v.as_str()) == Some("power")
                && !s.entity_id.contains("hourly")
                && !s.entity_id.contains("production")
                && !s.entity_id.contains("battery_power")
        })
        .filter_map(|s| {
            let w = parse_i32(&s.state)?;
            let name = friendly_name(s)
                .replace(" Current power", "")
                .replace(" Puissance Actuelle", "")
                .replace(" Puissance", "");
            Some(HaPowerPlug {
                entity_id: s.entity_id.clone(),
                name,
                power_w: w,
            })
        })
        .collect();

    plugs.sort_by(|a, b| b.power_w.cmp(&a.power_w).then_with(|| a.name.cmp(&b.name)));
    plugs.truncate(8);

    HaEnergy {
        solar_now_w,
        solar_today_kwh,
        forecast_today_kwh,
        forecast_tomorrow_kwh,
        plugs,
    }
}

fn extract_climate(by_id: &HashMap<&str, &HaRawState>) -> Vec<HaClimateSensor> {
    let mut out = Vec::new();
    let candidates = [
        (
            "sensor.miribel_temperature",
            Some("sensor.miribel_humidity_2"),
            "Extérieur (Miribel)",
            "outdoor",
        ),
        (
            "sensor.awtrix_3da598_temperature",
            Some("sensor.awtrix_3da598_humidity"),
            "Salon (Awtrix 1)",
            "indoor",
        ),
        (
            "sensor.awtrix_c02860_temperature",
            Some("sensor.awtrix_c02860_humidity"),
            "Bureau (Awtrix 2)",
            "indoor",
        ),
        ("sensor.piscine_temperature_2", None, "Piscine", "pool"),
    ];

    for (temp_id, hum_id, label, kind) in candidates {
        if let Some(t) = valid_state(by_id, temp_id) {
            let humidity_pct = hum_id
                .and_then(|hid| valid_state(by_id, hid))
                .and_then(|h| parse_u32(&h.state));
            out.push(HaClimateSensor {
                name: label.to_string(),
                temp_c: format_decimal(&t.state, 1),
                humidity_pct,
                kind: kind.to_string(),
            });
        }
    }
    out
}

fn extract_nas(by_id: &HashMap<&str, &HaRawState>) -> Option<HaNasStatus> {
    let cpu_pct = valid_state(by_id, "sensor.jonas_utilisation_du_processeur_totale")
        .and_then(|s| parse_u32(&s.state));
    let ram_pct = valid_state(by_id, "sensor.jonas_utilisation_de_la_memoire_reelle")
        .and_then(|s| parse_u32(&s.state));
    let volume_used_pct = valid_state(by_id, "sensor.jonas_volume_1_volume_utilise")
        .map(|s| format_decimal(&s.state, 1))
        .unwrap_or_else(|| "—".to_string());
    let status = valid_state(by_id, "sensor.jonas_volume_1_etat")
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut disk_temps_c = Vec::new();
    for id in [
        "sensor.jonas_drive_1_temperature",
        "sensor.jonas_drive_2_temperature",
    ] {
        if let Some(t) = valid_state(by_id, id).and_then(|s| parse_i32(&s.state)) {
            disk_temps_c.push(t);
        }
    }

    if cpu_pct.is_none() && ram_pct.is_none() && volume_used_pct == "—" {
        return None;
    }

    Some(HaNasStatus {
        name: "JoNAS (Synology)".to_string(),
        cpu_pct,
        ram_pct,
        volume_used_pct,
        disk_temps_c,
        status,
    })
}

fn extract_devices(by_id: &HashMap<&str, &HaRawState>, states: &[HaRawState]) -> HaDeviceSummary {
    let alarm_state = valid_state(by_id, "alarm_control_panel.ha_alarm")
        .or_else(|| valid_state(by_id, "alarm_control_panel.lumi_gateway_v3_alarm"))
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let vacuum_state = valid_state(by_id, "vacuum.robot_aspirateur")
        .map(|s| s.state.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let vacuum_battery_pct = valid_state(by_id, "sensor.cuisine_robot_aspirateur_batterie")
        .and_then(|s| parse_u32(&s.state));

    let covers: Vec<&HaRawState> = states
        .iter()
        .filter(|s| s.entity_id.starts_with("cover.") && s.state != "unavailable")
        .collect();
    let covers_total = covers.len() as u32;
    let open_cover_names: Vec<String> = covers
        .iter()
        .filter(|c| c.state == "open")
        .map(|c| friendly_name(c))
        .collect();
    let covers_open = open_cover_names.len() as u32;

    HaDeviceSummary {
        alarm_state,
        vacuum_state,
        vacuum_battery_pct,
        covers_open,
        covers_total,
        open_cover_names,
    }
}

fn format_decimal(raw: &str, decimals: usize) -> String {
    match raw.trim().parse::<f64>() {
        Ok(v) => format!("{v:.decimals$}"),
        Err(_) => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_ha_entities() {
        let json = r#"[
          {"entity_id":"person.joseph","state":"home","attributes":{"friendly_name":"Joseph"}},
          {"entity_id":"person.charlotte","state":"not_home","attributes":{"friendly_name":"Charlotte"}},
          {"entity_id":"sensor.volvo_ex40_batterie","state":"78","attributes":{"unit_of_measurement":"%"}},
          {"entity_id":"sensor.volvo_ex40_autonomie","state":"363","attributes":{"unit_of_measurement":"km"}},
          {"entity_id":"sensor.volvo_ex40_statut_charge","state":"Débranchée","attributes":{}},
          {"entity_id":"sensor.volvo_ex40_localisation","state":"Maison (Miribel)","attributes":{}},
          {"entity_id":"input_select.volvo_etat_verrouillage","state":"Verrouillée","attributes":{}},
          {"entity_id":"sensor.energy_production_today","state":"15.481","attributes":{"unit_of_measurement":"kWh"}},
          {"entity_id":"sensor.pw_23_terrabot_current_power","state":"88.4","attributes":{"device_class":"power","friendly_name":"pw-23_terrabot Current power"}},
          {"entity_id":"sensor.miribel_temperature","state":"9.7","attributes":{}},
          {"entity_id":"sensor.jonas_utilisation_du_processeur_totale","state":"47","attributes":{}},
          {"entity_id":"cover.volet_victor_shade_2","state":"open","attributes":{"friendly_name":"Volet Victor"}},
          {"entity_id":"cover.volet_alice_shade","state":"closed","attributes":{"friendly_name":"Volet Alice"}}
        ]"#;

        let parsed = parse_ha_states_json(json, 1_700_000_000_000);
        assert!(parsed.is_ok());
        if let Ok(data) = parsed {
            assert!(data.connected);
            assert_eq!(data.people.len(), 2);
            assert_eq!(data.vehicle.as_ref().and_then(|v| v.battery_pct), Some(78));
            assert_eq!(data.energy.forecast_today_kwh, "15.5");
            assert_eq!(data.energy.plugs.len(), 1);
            assert_eq!(data.energy.plugs[0].power_w, 88);
            assert_eq!(data.climate.len(), 1);
            assert_eq!(data.nas.as_ref().and_then(|n| n.cpu_pct), Some(47));
            assert_eq!(data.devices.covers_open, 1);
            assert_eq!(data.devices.covers_total, 2);
        }
    }
}
