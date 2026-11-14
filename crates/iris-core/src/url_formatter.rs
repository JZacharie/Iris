use crate::model::{DisplayScreen, ScreenType};

/// Formats dashboard URLs for embedded tablet kiosk display (Grafana, OpenObserve, Web).
pub struct DashboardUrlFormatter;

impl DashboardUrlFormatter {
    pub fn format_screen(screen: &DisplayScreen) -> String {
        Self::format_url(
            &screen.url,
            screen.screen_type,
            screen.kiosk_mode,
            screen.auto_refresh_seconds,
        )
    }

    pub fn format_url(
        raw_url: &str,
        screen_type: ScreenType,
        kiosk_mode: bool,
        auto_refresh_seconds: u32,
    ) -> String {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        match screen_type {
            ScreenType::Grafana => format_grafana_url(trimmed, kiosk_mode, auto_refresh_seconds),
            ScreenType::OpenObserve => format_openobserve_url(trimmed, kiosk_mode),
            ScreenType::Web => trimmed.to_string(),
            ScreenType::OrgOverview
            | ScreenType::OrgNotes
            | ScreenType::OrgClock
            | ScreenType::OrgAgenda
            | ScreenType::OrgStreamDeck => String::new(),
        }
    }
}

fn format_grafana_url(url: &str, kiosk_mode: bool, auto_refresh_seconds: u32) -> String {
    let mut result = url.to_string();
    if kiosk_mode && !has_query_param(&result, "kiosk") {
        result = append_query_param(&result, "kiosk", "tv");
    }
    if auto_refresh_seconds > 0 && !has_query_param(&result, "refresh") {
        let refresh = format!("{auto_refresh_seconds}s");
        result = append_query_param(&result, "refresh", &refresh);
    }
    result
}

fn format_openobserve_url(url: &str, kiosk_mode: bool) -> String {
    let mut result = url.to_string();
    if kiosk_mode && result.contains("/dashboards/") && !has_query_param(&result, "hide_header") {
        result = append_query_param(&result, "hide_header", "true");
    }
    result
}

fn has_query_param(url: &str, key: &str) -> bool {
    let after_q = match url.split_once('?') {
        Some((_, q)) => q,
        None => return false,
    };
    let query = after_q.split('#').next().unwrap_or(after_q);
    query
        .split('&')
        .any(|part| part.split('=').next().unwrap_or("") == key)
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let (base, fragment) = match url.find('#') {
        Some(idx) => (&url[..idx], &url[idx..]),
        None => (url, ""),
    };
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}{key}={value}{fragment}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafana_kiosk_and_refresh_appended() {
        let out = DashboardUrlFormatter::format_url(
            "https://grafana.zacharie.org/d/abc/cluster",
            ScreenType::Grafana,
            true,
            30,
        );
        assert_eq!(
            out,
            "https://grafana.zacharie.org/d/abc/cluster?kiosk=tv&refresh=30s"
        );
    }

    #[test]
    fn grafana_preserves_existing_query_and_fragment() {
        let out = DashboardUrlFormatter::format_url(
            "https://grafana.zacharie.org/d/abc/cluster?orgId=1#panel-2",
            ScreenType::Grafana,
            true,
            0,
        );
        assert_eq!(
            out,
            "https://grafana.zacharie.org/d/abc/cluster?orgId=1&kiosk=tv#panel-2"
        );
    }

    #[test]
    fn openobserve_adds_hide_header_for_dashboards() {
        let out = DashboardUrlFormatter::format_url(
            "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default",
            ScreenType::OpenObserve,
            true,
            0,
        );
        assert_eq!(
            out,
            "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&hide_header=true"
        );
    }
}
