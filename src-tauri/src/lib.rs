// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;
pub mod proxy;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let proxy_state = Arc::new(crate::proxy::state::ProxyState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(proxy_state)
        .setup(|_app| {
            // 启动时自动拉起代理(可以从 Settings 关闭,这里固定 on-by-default)
            // 注意: 这里的 auto_start 是开着的;若要关请在 Settings 加开关并读 ~/.codex/codex-box/config.json
            // 留给 v0.3.1 后续
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_summary,
            commands::config_snapshot::config_snapshot,
            commands::config_write::config_change_preview,
            commands::config_write::config_change_apply,
            commands::opencodex::opencodex_config_read,
            commands::opencodex::provider_route_upsert,
            commands::opencodex::provider_route_delete,
            commands::opencodex::catalog_entry_upsert,
            commands::opencodex::catalog_entry_delete,
            commands::proxy::proxy_status,
            commands::proxy::proxy_start,
            commands::proxy::proxy_stop,
            commands::proxy::proxy_restart,
            commands::proxy::proxy_models_preview,
            commands::proxy::proxy_inject_base_url_preview,
            commands::proxy::proxy_inject_base_url_apply,
            commands::proxy::proxy_restore_base_url_preview,
            commands::proxy::proxy_restore_base_url_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
