use std::time::Duration;

use iris_core::{GitHubPrSummary, IrisError, Result};
use serde::Deserialize;
use tracing::warn;

#[derive(Deserialize)]
struct GitHubPull {
    number: u64,
    title: String,
    html_url: String,
    user: Option<GitHubUser>,
    #[serde(default)]
    labels: Vec<GitHubLabel>,
}

#[derive(Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Deserialize)]
struct GitHubLabel {
    name: String,
}

pub struct GitHubClient {
    http: reqwest::Client,
    token: Option<String>,
}

impl GitHubClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("iris-server/1.0")
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        Self { http, token }
    }

    pub async fn fetch_open_prs(&self, repos: &[String]) -> Vec<GitHubPrSummary> {
        let mut all_prs = Vec::new();
        for repo in repos {
            let repo = repo.trim();
            if repo.is_empty() || !repo.contains('/') {
                continue;
            }
            match self.fetch_repo_prs(repo).await {
                Ok(mut prs) => all_prs.append(&mut prs),
                Err(err) => {
                    warn!(%err, repo, "Failed to fetch GitHub open PRs");
                }
            }
        }
        all_prs
    }

    async fn fetch_repo_prs(&self, repo: &str) -> Result<Vec<GitHubPrSummary>> {
        let url = format!("https://api.github.com/repos/{repo}/pulls?state=open&per_page=10");
        let mut req = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            IrisError::Persistence(format!("github request failed for {repo}: {e}"))
        })?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "github API for {repo} returned status {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("github body error for {repo}: {e}")))?;

        parse_prs_json(repo, &body)
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_prs_json(repo: &str, body: &str) -> Result<Vec<GitHubPrSummary>> {
    let pulls: Vec<GitHubPull> = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("invalid github PRs JSON: {e}")))?;

    Ok(pulls
        .into_iter()
        .map(|pr| {
            let author = pr
                .user
                .map(|u| u.login)
                .unwrap_or_else(|| "unknown".to_string());
            let author_lower = author.to_ascii_lowercase();
            let title_lower = pr.title.to_ascii_lowercase();
            let has_dep_label = pr.labels.iter().any(|l| {
                let name = l.name.to_ascii_lowercase();
                name.contains("dependenc") || name.contains("renovate")
            });
            let is_dependency_update = author_lower.contains("renovate")
                || author_lower.contains("dependabot")
                || has_dep_label
                || title_lower.starts_with("chore(deps)")
                || title_lower.starts_with("build(deps)")
                || title_lower.starts_with("update dependency");

            GitHubPrSummary {
                repo: repo.to_string(),
                number: pr.number,
                title: pr.title,
                author,
                url: pr.html_url,
                is_dependency_update,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::parse_prs_json;

    #[test]
    fn parses_prs_and_flags_dependency_updates() {
        let json = r#"[
            {
                "number": 42,
                "title": "chore(deps): update argocd helm chart to 7.8.0",
                "html_url": "https://github.com/JZacharie/jo3/pull/42",
                "user": { "login": "renovate[bot]" },
                "labels": [{ "name": "dependencies" }]
            },
            {
                "number": 43,
                "title": "feat: add new dashboard",
                "html_url": "https://github.com/JZacharie/jo3/pull/43",
                "user": { "login": "JZacharie" },
                "labels": []
            }
        ]"#;

        let prs = parse_prs_json("JZacharie/jo3", json).unwrap();
        assert_eq!(prs.len(), 2);
        assert!(prs[0].is_dependency_update);
        assert_eq!(prs[0].author, "renovate[bot]");
        assert!(!prs[1].is_dependency_update);
        assert_eq!(prs[1].number, 43);
    }
}
