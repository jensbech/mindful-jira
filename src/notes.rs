use std::collections::HashMap;
use std::fs;
use std::path::Path;

const NOTES_FILE: &str = "notes.json";

pub fn load_notes() -> HashMap<String, String> {
    let path = Path::new(NOTES_FILE);
    if !path.exists() {
        return HashMap::new();
    }
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_notes(notes: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string_pretty(notes) {
        let _ = fs::write(NOTES_FILE, json);
    }
}
