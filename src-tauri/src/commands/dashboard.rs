// src-tauri/src/commands/dashboard.rs
use crate::config::model::DashboardSummary;
use crate::config::{loader, parser};
use crate::error::{AppError, AppResult};
use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";

fn resolve_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(DEFAULT_CONFIG_PATH))
}

#[tauri::command]
pub fn dashboard_summary() -> AppResult<DashboardSummary> {
    let path = resolve_config_path()?;
    let raw = loader::read_raw(&path)?;
    let config = parser::parse(&raw)?;
    Ok(parser::to_dashboard_summary(&config))
}
