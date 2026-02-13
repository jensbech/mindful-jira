use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Deserialize, Serialize, Clone)]
pub struct StatusFilter {
    pub name: String,
    /// true = this status is hidden from results
    #[serde(alias = "enabled")]
    pub excluded: bool,
}

fn default_status_filters() -> Vec<StatusFilter> {
    [
        "Backlog",
        "Done",
        "Closed",
        "Resolved",
        "Rejected",
        "Approved Requirement",
    ]
    .iter()
    .map(|s| StatusFilter {
        name: s.to_string(),
        excluded: true,
    })
    .collect()
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub jira_url: String,
    pub email: String,
    pub api_token: String,
    #[serde(default = "default_status_filters")]
    pub status_filters: Vec<StatusFilter>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = Path::new("jira-config.json");
        if !path.exists() {
            return Err(
                "jira-config.json not found. Copy jira-config.json.example to jira-config.json and fill in your credentials.".to_string()
            );
        }
        let contents = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read jira-config.json: {e}"))?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse jira-config.json: {e}"))
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write("jira-config.json", json);
        }
    }

    pub fn excluded_status_names(&self) -> Vec<&str> {
        self.status_filters
            .iter()
            .filter(|sf| sf.excluded)
            .map(|sf| sf.name.as_str())
            .collect()
    }
}
