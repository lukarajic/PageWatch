use crate::models::Watch;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const STORAGE_FILE: &str = "watches.json";

pub fn load_watches() -> Result<Vec<Watch>> {
    if !Path::new(STORAGE_FILE).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(STORAGE_FILE)
        .context("Failed to read storage file")?;
    
    let watches: Vec<Watch> = serde_json::from_str(&content)
        .context("Failed to parse storage file")?;
    
    Ok(watches)
}

pub fn save_watches(watches: &[Watch]) -> Result<()> {
    let content = serde_json::to_string_pretty(watches)
        .context("Failed to serialize watches")?;
    
    fs::write(STORAGE_FILE, content)
        .context("Failed to write storage file")?;
    
    Ok(())
}
