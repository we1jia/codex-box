// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;
pub mod proxy;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let proxy_state = Arc::new(crate::proxy::state::ProxyState::new());
    let proxy_state_for_setup = proxy_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(proxy_state)
        .setup(move |_app| {
            // 启动时自动拉起代理(可以从 Settings 关闭,这里固定 on-by-default)
            // 注意: 这里的 auto_start 是开着的;若要关请在 Settings 加开关并读 ~/.codex/codex-box/config.json
            // 留给 v0.3.1 后续
            let state = proxy_state_for_setup.clone();
            tauri::async_runtime::spawn(async move {
                let map = crate::proxy::inject_map::read_inject_map()
                    .unwrap_or_else(|_| Default::default());
                state.set_inject_map(map);
                state.log_event("info", "runtime", "Codex Box auto-start requested");
                if let Err(err) =
                    crate::proxy::lifecycle::start(state, crate::proxy::DEFAULT_PROXY_PORT).await
                {
                    tracing::warn!("proxy auto-start failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_summary,
            commands::effective_routing::effective_routing_status,
            commands::history::codex_history_reconcile,
            commands::history::codex_history_unify_preview,
            commands::history::codex_history_unify_apply,
            commands::config_snapshot::config_snapshot,
            commands::config_write::config_change_preview,
            commands::config_write::config_change_apply,
            commands::conversation_provider::conversation_provider_candidates,
            commands::conversation_provider::conversation_provider_preview,
            commands::conversation_provider::conversation_provider_apply,
            commands::conversation_provider::byok_activation_preview,
            commands::conversation_provider::byok_activation_apply,
            commands::opencodex::opencodex_config_read,
            commands::opencodex::config_import_sources_scan,
            commands::opencodex::opencodex_import_preview,
            commands::opencodex::opencodex_import_apply,
            commands::opencodex::provider_route_upsert,
            commands::opencodex::provider_route_delete,
            commands::opencodex::catalog_entry_upsert,
            commands::opencodex::catalog_entry_delete,
            commands::opencodex::simple_model_config_save,
            commands::opencodex::codex_multirouter_preview,
            commands::opencodex::codex_multirouter_apply,
            commands::opencodex::codex_multirouter_sync,
            commands::opencodex::codex_models_cache_restore_preview,
            commands::opencodex::codex_models_cache_restore_apply,
            commands::proxy::proxy_status,
            commands::proxy::proxy_start,
            commands::proxy::proxy_stop,
            commands::proxy::proxy_restart,
            commands::proxy::proxy_models_preview,
            commands::proxy::proxy_route_test,
            commands::proxy::proxy_inject_base_url_preview,
            commands::proxy::proxy_inject_base_url_apply,
            commands::proxy::proxy_restore_base_url_preview,
            commands::proxy::proxy_restore_base_url_apply,
            commands::codex_desktop::codex_desktop_picker_unlock,
            commands::codex_desktop::codex_desktop_launch_with_debugging_and_unlock,
            commands::system::codex_runtime_status,
            commands::system::codex_desktop_integration_status,
            commands::system::open_path,
            commands::system::reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
