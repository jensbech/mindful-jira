use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone)]
pub struct StatusFilter {
    pub name: String,
    /// true = this status is hidden from results
    #[serde(alias = "enabled")]
    pub excluded: bool,
}

pub fn default_status_filters() -> Vec<StatusFilter> {
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
    #[serde(default)]
    pub sort_order: Option<String>,
}

pub fn config_dir() -> PathBuf {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("mindful-jira");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = config_path();
        if !path.exists() {
            return Err(
                "Config not found. Run `mindful-jira setup` to configure.".to_string()
            );
        }
        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    }

    pub fn save(&self) {
        let path = config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
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
