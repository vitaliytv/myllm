mod omlx;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_agent::init())
        .manage(omlx::admin::OmlxState::default())
        .manage(omlx::proxy::ProxyRuntime::default())
        .invoke_handler(tauri::generate_handler![
            omlx::admin::omlx_env_api_key,
            omlx::admin::omlx_connect,
            omlx::admin::omlx_stats,
            omlx::admin::omlx_global_settings,
            omlx::proxy::proxy_start,
            omlx::proxy::proxy_stop,
            omlx::proxy::proxy_history
        ]);

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
