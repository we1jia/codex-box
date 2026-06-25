// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;

use commands::opencodex::OpenCodexManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(OpenCodexManager::default())
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_summary,
            commands::config_snapshot::config_snapshot,
            commands::config_write::config_change_preview,
            commands::config_write::config_change_apply,
            commands::opencodex::opencodex_status,
            commands::opencodex::opencodex_start,
            commands::opencodex::opencodex_start_lan,
            commands::opencodex::opencodex_stop,
            commands::opencodex::opencodex_restart,
            commands::opencodex::opencodex_restart_lan,
            commands::opencodex::opencodex_open_url,
            commands::opencodex::opencodex_open_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
