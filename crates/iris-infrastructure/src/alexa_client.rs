use std::time::Duration;

use iris_core::{IrisError, Result, ShoppingItem};
use serde::Deserialize;

const DEFAULT_ALEXA_MCP_URL: &str = "http://alexa-mcp.alexa-mcp.svc.cluster.local:8080";

#[derive(Deserialize)]
struct AlexaItemsResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    items: Vec<AlexaRawItem>,
}

#[derive(Deserialize)]
struct AlexaRawItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    completed: bool,
}

pub struct AlexaMcpClient {
    base_url: String,
    http: reqwest::Client,
}

impl AlexaMcpClient {
    pub fn new() -> Self {
        let base_url = std::env::var("ALEXA_MCP_URL")
            .ok()
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| DEFAULT_ALEXA_MCP_URL.to_string());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { base_url, http }
    }

    pub async fn fetch_shopping_list(&self) -> Result<Vec<ShoppingItem>> {
        let url = format!("{}/api/alexa/items", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("alexa-mcp request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "alexa-mcp returned status {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("alexa-mcp body error: {e}")))?;

        parse_alexa_items_json(&body)
    }
}

impl Default for AlexaMcpClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_alexa_items_json(body: &str) -> Result<Vec<ShoppingItem>> {
    let parsed: AlexaItemsResponse = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("invalid alexa-mcp JSON: {e}")))?;

    if !parsed.success {
        return Err(IrisError::Persistence(
            "alexa-mcp returned success=false".to_string(),
        ));
    }

    Ok(parsed
        .items
        .into_iter()
        .filter(|i| !i.value.trim().is_empty())
        .enumerate()
        .map(|(idx, item)| ShoppingItem {
            id: if item.id.trim().is_empty() {
                format!("alexa-{idx}")
            } else {
                item.id
            },
            name: item.value,
            completed: item.completed,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::parse_alexa_items_json;

    #[test]
    fn parses_alexa_shopping_list_items() {
        let json = r#"{
            "success": true,
            "items": [
                { "id": "item-1", "value": "Lait demi-écrémé", "completed": false },
                { "id": "item-2", "value": "Café", "completed": true }
            ]
        }"#;

        let items = parse_alexa_items_json(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Lait demi-écrémé");
        assert!(!items[0].completed);
        assert!(items[1].completed);
    }
}
