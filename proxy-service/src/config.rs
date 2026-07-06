//! Конфігурація headless-сервісу — усі параметри через env (нема GUI-форми,
//! на відміну від колишньої картки «Підключення» в myllm). Дефолти дзеркалять
//! те, що раніше жило в `services/omlx-connection.js`.

use std::path::PathBuf;

/// Дефолтний upstream (прямий omlx, без проксі-в-проксі).
const DEFAULT_UPSTREAM_BASE_URL: &str = "http://127.0.0.1:8000";
/// Дефолтний порт локального проксі (той самий, що був у Tauri-версії).
const DEFAULT_PORT: u16 = 8088;

#[derive(Debug, Clone)]
pub struct Config {
    /// Адреса справжнього omlx-сервера, куди форвардиться `/v1/*` трафік.
    pub upstream_base_url: String,
    /// Порт, на якому слухає локальний зворотний проксі.
    pub port: u16,
    /// API-ключ для admin-сесії (login+stats+global-settings) — той самий
    /// `OMLX_API_KEY`, яким користується `~/.claude/mcp-omlx.mjs`. `None` —
    /// admin-сесія не автопіднімається при старті, лишається доступною через
    /// `POST /_proxy/admin/connect` вручну.
    pub api_key: Option<String>,
    /// Директорія стору (`requests.jsonl`) — аналог колишнього Tauri
    /// `app_data_dir()`.
    pub data_dir: PathBuf,
}

impl Config {
    /// Читає конфіг з env; best-effort дефолти, ніколи не падає.
    pub fn from_env() -> Self {
        Self {
            upstream_base_url: std::env::var("MYLLM_PROXY_UPSTREAM_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_UPSTREAM_BASE_URL.to_string()),
            port: std::env::var("MYLLM_PROXY_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_PORT),
            api_key: std::env::var("OMLX_API_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
            data_dir: std::env::var("MYLLM_PROXY_DATA_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(default_data_dir),
        }
    }
}

/// `~/Library/Application Support/myllm-proxy-service` на macOS (той самий
/// каталог-стиль, що Tauri `app_data_dir()` використовував для `myllm`).
fn default_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("myllm-proxy-service")
}
