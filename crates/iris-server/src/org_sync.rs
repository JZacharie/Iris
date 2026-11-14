use std::sync::Arc;
use std::time::Duration;

use iris_application::IrisController;
use iris_infrastructure::{IcalClient, OpenMeteoClient};
use tracing::warn;

pub fn spawn_org_sync_loop(controller: Arc<IrisController>) {
    tokio::spawn(async move {
        let weather = OpenMeteoClient::new();
        let ical = IcalClient::new();
        loop {
            refresh_once(&controller, &weather, &ical).await;
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

pub async fn refresh_org_now(controller: Arc<IrisController>) {
    let weather = OpenMeteoClient::new();
    let ical = IcalClient::new();
    refresh_once(&controller, &weather, &ical).await;
}

async fn refresh_once(controller: &IrisController, weather: &OpenMeteoClient, ical: &IcalClient) {
    let org_cfg = controller.snapshot().await.settings.org;

    match weather.fetch_3_days(&org_cfg).await {
        Ok(forecast) => {
            controller.set_weather(forecast).await;
        }
        Err(err) => {
            warn!(%err, city = %org_cfg.weather_city, "Failed to fetch 3-day weather");
        }
    }

    if !org_cfg.ical_url.trim().is_empty() {
        match ical.fetch_week_events(&org_cfg.ical_url).await {
            Ok(events) => {
                controller.set_ical_events(events).await;
            }
            Err(err) => {
                warn!(%err, "Failed to fetch iCal weekly planning");
            }
        }
    }
}
