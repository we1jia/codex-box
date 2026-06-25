// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_summary,
            commands::config_snapshot::config_snapshot,
            commands::config_write::config_change_preview,
            commands::config_write::config_change_apply,
            commands::opencodex::opencodex_config_read,
            commands::opencodex::provider_route_upsert,
            commands::opencodex::provider_route_delete,
            commands::opencodex::catalog_entry_upsert,
            commands::opencodex::catalog_entry_delete
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}