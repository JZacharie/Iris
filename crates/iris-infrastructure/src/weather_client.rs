use chrono::Utc;
use iris_core::{IrisError, OrgConfig, Result, WeatherDay, WeatherForecast};
use serde::Deserialize;

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: Option<CurrentWeather>,
    daily: Option<DailyWeather>,
}

#[derive(Deserialize)]
struct CurrentWeather {
    temperature_2m: f64,
    weather_code: u32,
}

#[derive(Deserialize)]
struct DailyWeather {
    time: Vec<String>,
    weather_code: Vec<u32>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    precipitation_probability_max: Option<Vec<u32>>,
    wind_speed_10m_max: Option<Vec<f64>>,
}

pub struct OpenMeteoClient {
    http: reqwest::Client,
}

impl OpenMeteoClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    pub async fn fetch_3_days(&self, org: &OrgConfig) -> Result<WeatherForecast> {
        let lat = org.weather_lat.trim();
        let lon = org.weather_lon.trim();
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,wind_speed_10m_max&timezone=Europe%2FParis&forecast_days=3"
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("weather fetch: {e}")))?
            .error_for_status()
            .map_err(|e| IrisError::Persistence(format!("weather status: {e}")))?
            .json::<OpenMeteoResponse>()
            .await
            .map_err(|e| IrisError::Persistence(format!("weather json: {e}")))?;

        let (current_temp_c, current_code) = match resp.current {
            Some(c) => (c.temperature_2m.round() as i32, c.weather_code),
            None => (0, 0),
        };
        let (current_label, current_icon) = map_wmo_code(current_code);

        let mut days = Vec::new();
        if let Some(daily) = resp.daily {
            for (idx, date) in daily.time.iter().enumerate() {
                let code = daily.weather_code.get(idx).copied().unwrap_or(0);
                let (label, icon) = map_wmo_code(code);
                let temp_max = daily
                    .temperature_2m_max
                    .get(idx)
                    .copied()
                    .unwrap_or(0.0)
                    .round() as i32;
                let temp_min = daily
                    .temperature_2m_min
                    .get(idx)
                    .copied()
                    .unwrap_or(0.0)
                    .round() as i32;
                let precip = daily
                    .precipitation_probability_max
                    .as_ref()
                    .and_then(|v| v.get(idx))
                    .copied()
                    .unwrap_or(0);
                let wind = daily
                    .wind_speed_10m_max
                    .as_ref()
                    .and_then(|v| v.get(idx))
                    .copied()
                    .unwrap_or(0.0)
                    .round() as u32;

                days.push(WeatherDay {
                    date: date.clone(),
                    label: label.to_string(),
                    icon: icon.to_string(),
                    temp_min_c: temp_min,
                    temp_max_c: temp_max,
                    precip_prob_pct: precip,
                    wind_max_kmh: wind,
                });
            }
        }

        Ok(WeatherForecast {
            city: org.weather_city.clone(),
            current_temp_c,
            current_label: current_label.to_string(),
            current_icon: current_icon.to_string(),
            days,
            updated_at_epoch_ms: Utc::now().timestamp_millis(),
        })
    }
}

impl Default for OpenMeteoClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn map_wmo_code(code: u32) -> (&'static str, &'static str) {
    match code {
        0 => ("Ensoleillé", "☀️"),
        1 | 2 => ("Éclaircies", "⛅"),
        3 => ("Couvert", "☁️"),
        45 | 48 => ("Brouillard", "🌫️"),
        51 | 53 | 55 | 56 | 57 => ("Bruine", "🌦️"),
        61 | 63 | 65 | 66 | 67 | 80..=82 => ("Pluie", "🌧️"),
        71 | 73 | 75 | 77 | 85 | 86 => ("Neige", "❄️"),
        95 | 96 | 99 => ("Orage", "⛈️"),
        _ => ("Variable", "🌤️"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wmo_codes_to_french_labels() {
        assert_eq!(map_wmo_code(0), ("Ensoleillé", "☀️"));
        assert_eq!(map_wmo_code(63), ("Pluie", "🌧️"));
        assert_eq!(map_wmo_code(95), ("Orage", "⛈️"));
    }
}
