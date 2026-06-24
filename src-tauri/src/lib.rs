// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_summary
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
