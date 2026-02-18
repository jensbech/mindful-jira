use std::collections::HashMap;
use std::fs;

use crate::config;

fn notes_path() -> std::path::PathBuf {
    config::config_dir().join("notes.json")
}

pub fn load_notes() -> HashMap<String, String> {
    let path = notes_path();
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_notes(notes: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string_pretty(notes) {
        let _ = fs::write(notes_path(), json);
    }
}

fn long_notes_path() -> std::path::PathBuf {
    config::config_dir().join("long_notes.json")
}

pub fn load_long_notes() -> HashMap<String, String> {
    let path = long_notes_path();
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_long_notes(notes: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string_pretty(notes) {
        let _ = fs::write(long_notes_path(), json);
    }
}

fn highlight_path() -> std::path::PathBuf {
    config::config_dir().join("highlights.json")
}

pub fn load_highlights() -> HashMap<String, String> {
    let contents = match fs::read_to_string(highlight_path()) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    // Try loading as new format (HashMap<String, String>)
    if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&contents) {
        return map;
    }
    // Migrate from old format (HashSet<String>) — treat all as "yellow"
    if let Ok(set) = serde_json::from_str::<std::collections::HashSet<String>>(&contents) {
        let map: HashMap<String, String> = set.into_iter().map(|k| (k, "yellow".to_string())).collect();
        save_highlights(&map);
        return map;
    }
    HashMap::new()
}

pub fn save_highlights(keys: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string(keys) {
        let _ = fs::write(highlight_path(), json);
    }
}

fn muted_path() -> std::path::PathBuf {
    config::config_dir().join("muted.json")
}

pub fn load_muted() -> std::collections::HashSet<String> {
    let contents = match fs::read_to_string(muted_path()) {
        Ok(c) => c,
        Err(_) => return std::collections::HashSet::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_muted(keys: &std::collections::HashSet<String>) {
    if let Ok(json) = serde_json::to_string(keys) {
        let _ = fs::write(muted_path(), json);
    }
}
