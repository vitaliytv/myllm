mod omlx;
mod pi_agent;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_agent::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            omlx::chains::chains_list,
            omlx::chains::chain_steps,
            omlx::chains::save_chain_analysis,
            omlx::chains::list_chain_analyses,
            omlx::chains::read_body_capture,
            pi_agent::pi_agent_models,
            pi_agent::run_pi_agent
        ]);

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    // relaunch() після встановлення оновлення — щоб застосунок сам
    // перезапустився в нову версію, а не чекав ручного рестарту.
    let builder = builder.plugin(tauri_plugin_process::init());

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder
        .setup(|app| {
            // Версія застосунку в заголовку вікна, щоб її було видно без About-діалогу
            #[cfg(desktop)]
            if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                let _ = window.set_title(&format!(
                    "myllm — omlx queue v{}",
                    app.package_info().version
                ));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
