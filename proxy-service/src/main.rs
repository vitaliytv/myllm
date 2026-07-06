//! Безінтерфейсний (headless) зворотний проксі omlx — витягнуто з Tauri-
//! застосунку myllm (spec: винесення проксі в окремий launchd-сервіс), щоб
//! форвардинг+компресія+історія+admin-стати+кореляція ланцюжків жили в
//! процесі, що персистентно працює на маку без GUI (launchd LaunchAgent,
//! див. `launchd/`), а не лише поки відкрите вікно застосунку.
//!
//! Конфіг — лише env (`config.rs`): `MYLLM_PROXY_UPSTREAM_URL`,
//! `MYLLM_PROXY_PORT`, `OMLX_API_KEY`, `MYLLM_PROXY_DATA_DIR`.

mod admin;
mod chain_correlation;
mod client_info;
mod compress;
mod config;
mod proxy;

use config::Config;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let config = Config::from_env();
    eprintln!(
        "[myllm-proxy-service] upstream={} data_dir={}",
        config.upstream_base_url,
        config.data_dir.display()
    );

    let (port, listener, router, state) = match proxy::bind(&config).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[myllm-proxy-service] fatal: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[myllm-proxy-service] listening on 127.0.0.1:{port}");

    proxy::maybe_auto_connect_admin(&config, &state).await;

    if let Err(e) = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        eprintln!("[myllm-proxy-service] serve error: {e}");
        std::process::exit(1);
    }
}
