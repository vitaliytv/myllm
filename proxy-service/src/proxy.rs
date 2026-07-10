//! Зворотний проксі для `/v1/*` трафіку omlx + маленький локальний
//! `/_proxy/*` admin-API (parity з колишнім Tauri UI — без нього самого).
//!
//! Admin API upstream-сервера (`admin.rs`) дає тільки поточний стан черги —
//! після завершення запиту текст промпту/відповіді зникає. Щоб бачити повну
//! історію (хто що питав і що відповів), клієнти (mlmail, myshare,
//! `mcp-omlx.mjs`) звертаються на локальний порт цього проксі замість прямого
//! `:8000`, проксі пересилає запит на справжній omlx один в один і паралельно
//! стрімить копію в лог — так стрімінг (SSE) не ламається для клієнта, а
//! історія запитів наповнюється текстом у реальному часі.
//!
//! Headless-адаптація колишнього Tauri `proxy.rs`: `ProxyShared.app` (Tauri
//! AppHandle) замінено на прямий `data_dir: PathBuf`; `proxy_start`/`proxy_stop`
//! (Tauri-команди з UI-кнопками) зникли — сервіс просто слухає, доки живий
//! процес (launchd керує рестартом); `omlx-request-logged` Tauri-подія
//! замінена на `/_proxy/history` (GET/DELETE) — той самий `requests.jsonl`,
//! просто без push-нотифікації (немає кому її слухати без GUI).

use crate::admin::{self, AdminState};
use crate::chain_correlation::extract_correlation;
use crate::client_info::{self, ClientInfo};
use crate::compress::compress_request_body;
use crate::config::Config;
use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::{any, get, post},
    Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;

/// Скільки символів тексту відповіді зберігаємо в записі — запобіжник проти
/// того, щоб один величезний non-JSON/non-SSE response роздув requests.jsonl.
const MAX_RESPONSE_CHARS: usize = 200_000;
/// Скільки останніх рядків читаємо з requests.jsonl для `/_proxy/history`.
const DEFAULT_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to bind {0}: {1}")]
    Bind(String, std::io::Error),
}

impl From<ProxyError> for String {
    fn from(value: ProxyError) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogEntry {
    pub id: u64,
    pub timestamp_ms: u64,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub duration_ms: u64,
    pub model: Option<String>,
    pub request_headers: Value,
    pub request_body: Option<Value>,
    pub response_text: String,
    /// Процес-клієнт (PID/назва/бінарник/cwd), зарезолвлений за портом
    /// з'єднання. `None` — коли процес не знайшовся (встиг завершитись) або
    /// платформа без підтримки резолву.
    pub client: Option<ClientInfo>,
    /// Чи стиснули `messages` перед форвардом на upstream (minify вбудованого
    /// JSON + truncation старих великих блоків, див. `compress.rs`). `false`
    /// також коли запит просто не підпадав під формат (tool-calls,
    /// response_format, не chat-completions) — компресія свідомо пропущена.
    pub prompt_compressed: bool,
    /// Кореляція з ланцюжками `@7n/llm-lib` (заголовок `x-chain-id`).
    /// `skip_serializing_if` — старі/некорельовані записи без null-шуму.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Номер кроку в ланцюжку (`x-chain-step`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_step: Option<u32>,
    /// Тип задачі ланцюжка (`x-chain-kind`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_kind: Option<String>,
    /// Директорія виклику клієнта (`x-chain-cwd`, декодована) — без
    /// process-інтроспекції.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_cwd: Option<String>,
    /// Fallback-джойн із trace llm-lib: sha256 hex16 останнього user-повідомлення
    /// (контракт у `chain_correlation.rs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_hash: Option<String>,
}

/// Спільний стан для всіх axum-хендлерів (proxy-форвард + `/_proxy/*` admin).
pub struct AppState {
    client: reqwest::Client,
    upstream_base_url: String,
    log_path: PathBuf,
    next_id: AtomicU64,
    /// Фактичний порт, на якому слухає проксі — потрібен для резолву
    /// процесу-клієнта за парою портів TCP-з'єднання.
    port: u16,
    admin: AdminState,
}

/// Піднімає TCP-listener і повертає готовий до `axum::serve` router разом із
/// фактичним портом (може відрізнятись від запитаного, якщо `config.port`
/// зайнятий і осі вибрано `0` — тут завжди точний порт, `bind` падає, якщо він
/// зайнятий, щоб не мовчки слухати не той порт).
pub async fn bind(
    config: &Config,
) -> Result<(u16, TcpListener, Router, Arc<AppState>), ProxyError> {
    std::fs::create_dir_all(&config.data_dir)?;
    let log_path = config.data_dir.join("requests.jsonl");

    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| ProxyError::Bind(addr, e))?;
    let actual_port = listener.local_addr()?.port();

    let state = Arc::new(AppState {
        client: reqwest::Client::new(),
        upstream_base_url: config.upstream_base_url.clone(),
        log_path,
        next_id: AtomicU64::new(0),
        port: actual_port,
        admin: AdminState::default(),
    });

    let router = Router::new()
        .route(
            "/_proxy/history",
            get(history_handler).delete(clear_history_handler),
        )
        .route("/_proxy/admin/connect", post(admin_connect_handler))
        .route("/_proxy/admin/stats", get(admin_stats_handler))
        .route(
            "/_proxy/admin/global-settings",
            get(admin_global_settings_handler),
        )
        .fallback(any(proxy_handler))
        .with_state(state.clone());

    Ok((actual_port, listener, router, state))
}

/// Best-effort admin-логін при старті, якщо `config.api_key` заданий (env
/// `OMLX_API_KEY`) — той самий UX, що колишній Tauri UI мав через
/// "Підключити": одразу піднята сесія, без ручного виклику
/// `/_proxy/admin/connect`. Помилка логіну НЕ валить сервіс — форвардинг
/// трафіку не залежить від admin-сесії.
pub async fn maybe_auto_connect_admin(config: &Config, state: &Arc<AppState>) {
    let Some(api_key) = &config.api_key else {
        return;
    };
    match admin::connect(&config.upstream_base_url, api_key).await {
        Ok(session) => *state.admin.0.lock().unwrap() = Some(session),
        Err(e) => eprintln!("[myllm-proxy-service] admin auto-connect failed: {e}"),
    }
}

async fn history_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<HistoryQuery>,
) -> Result<Json<Vec<Value>>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    read_tail_entries(&state.log_path, limit)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

async fn clear_history_handler(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    match tokio::fs::remove_file(&state.log_path).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Deserialize)]
struct ConnectBody {
    base_url: String,
    api_key: String,
}

async fn admin_connect_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConnectBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    match admin::connect(&body.base_url, &body.api_key).await {
        Ok(session) => {
            *state.admin.0.lock().unwrap() = Some(session);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => Err((StatusCode::BAD_GATEWAY, e.to_string())),
    }
}

async fn admin_stats_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    admin::stats(&state.admin)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

async fn admin_global_settings_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    admin::global_settings(&state.admin)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

async fn read_tail_entries(log_path: &PathBuf, limit: usize) -> Result<Vec<Value>, ProxyError> {
    let content = match tokio::fs::read_to_string(log_path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..]
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect())
}

/// Заголовки, які не пересилаємо вгору (hop-by-hop / такі, що reqwest
/// виставляє сам) — інакше content-length/host з клієнта конфліктує з тим,
/// що реально йде на upstream.
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host" | "content-length" | "connection" | "transfer-encoding"
    )
}

/// Копія заголовків для логу — без Authorization/API-ключа.
fn redact_headers(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        let key = name.as_str();
        if key.eq_ignore_ascii_case("authorization") || key.eq_ignore_ascii_case("x-api-key") {
            continue;
        }
        if let Ok(v) = value.to_str() {
            map.insert(key.to_string(), Value::String(v.to_string()));
        }
    }
    Value::Object(map)
}

async fn proxy_handler(
    State(shared): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    // Резолвимо процес-клієнт одразу (поки TCP-з'єднання ще відкрите і є в
    // таблиці сокетів) у blocking-пулі паралельно з походом на upstream;
    // результат забирається при фіналізації запису.
    let client_task = tokio::task::spawn_blocking({
        let client_port = peer.port();
        let proxy_port = shared.port;
        move || client_info::resolve(client_port, proxy_port)
    });
    let path = uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let url = format!("{}{path}", shared.upstream_base_url);

    let request_body: Option<Value> = serde_json::from_slice(&body).ok();
    let model = request_body
        .as_ref()
        .and_then(|v| v.get("model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let request_headers = redact_headers(&headers);
    // Кореляція з ланцюжками llm-lib: x-chain-* заголовки + prompt_hash з
    // ОРИГІНАЛЬНОГО тіла (до компресії — клієнт хешує те, що надіслав).
    let correlation = extract_correlation(&headers, request_body.as_ref());

    // Компресуємо тіло, що йде на upstream, окремо від `request_body`, який
    // лишається оригіналом для логу історії — так видно і що прислав
    // клієнт, і чи спрацювала компресія (`prompt_compressed`).
    let original_size = body.len();
    let (upstream_body, prompt_compressed) = match request_body
        .as_ref()
        .and_then(|v| compress_request_body(v, original_size))
        .and_then(|compressed| serde_json::to_vec(&compressed).ok())
    {
        Some(bytes) => (Bytes::from(bytes), true),
        None => (body.clone(), false),
    };

    let mut upstream_req = shared.client.request(method.clone(), &url);
    for (name, value) in headers.iter() {
        if !is_hop_by_hop(name) {
            upstream_req = upstream_req.header(name, value);
        }
    }
    let upstream_resp = match upstream_req.body(upstream_body).send().await {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("omlx upstream error: {e}")).into_response();
        }
    };

    let status = upstream_resp.status();
    let content_type = upstream_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mut resp_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers().iter() {
        if !is_hop_by_hop(name) {
            resp_headers.insert(name.clone(), value.clone());
        }
    }

    // Стрімимо тіло клієнту чанк-за-чанком через канал, і одночасно
    // накопичуємо копію байтів. Коли upstream-потік завершується, фіналізуємо
    // запис історії, коли довгий SSE не блокує клієнта.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let shared_for_task = shared.clone();
    let request_body_for_task = request_body.clone();
    // /health — це лише liveness-пінг клієнтів; логувати його в історію
    // запитів не має сенсу, він тільки засмічує список реальними чат-запитами.
    let should_log = path != "/health";
    tokio::spawn(async move {
        let mut stream = upstream_resp.bytes_stream();
        let mut captured: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    captured.extend_from_slice(&bytes);
                    if tx.send(Ok(bytes)).await.is_err() {
                        return; // клієнт відключився — лог все одно допишемо нижче
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                    break;
                }
            }
        }
        if !should_log {
            return;
        }
        let response_text = extract_response_text(&content_type, &captured);
        let client = client_task.await.ok().flatten();
        let entry = RequestLogEntry {
            id: shared_for_task.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64,
            method: method.to_string(),
            path,
            status: status.as_u16(),
            duration_ms: started.elapsed().as_millis() as u64,
            model,
            request_headers,
            request_body: request_body_for_task,
            response_text,
            client,
            prompt_compressed,
            correlation_id: correlation.correlation_id,
            chain_step: correlation.chain_step,
            chain_kind: correlation.chain_kind,
            chain_cwd: correlation.chain_cwd,
            prompt_hash: correlation.prompt_hash,
        };
        finalize_entry(&shared_for_task, entry).await;
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    response
}

async fn finalize_entry(shared: &AppState, entry: RequestLogEntry) {
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&shared.log_path)
            .await
        {
            use tokio::io::AsyncWriteExt;
            let _ = file.write_all(line.as_bytes()).await;
            let _ = file.write_all(b"\n").await;
        }
    }
}

/// Витягує читабельний текст відповіді з тіла: SSE (`text/event-stream`)
/// склеюється по `delta.content` чанках, звичайний JSON — по
/// `choices[0].message.content` (chat) з фолбеком на кілька інших відомих
/// полів omlx-подібних API. Що не розпізналось — лишається як raw-текст,
/// обрізаний до MAX_RESPONSE_CHARS.
fn extract_response_text(content_type: &str, raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let result = if content_type.contains("text/event-stream") {
        extract_sse_text(&text)
    } else {
        extract_json_text(&text).unwrap_or_else(|| text.to_string())
    };
    if result.chars().count() > MAX_RESPONSE_CHARS {
        result.chars().take(MAX_RESPONSE_CHARS).collect()
    } else {
        result
    }
}

fn extract_sse_text(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        if let Some(delta) = value
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
        {
            out.push_str(delta);
        }
    }
    out
}

fn extract_json_text(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    if let Some(content) = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        return Some(content.to_string());
    }
    if let Some(content) = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|c| c.as_str())
    {
        return Some(content.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_sse_delta_text() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                   data: [DONE]\n\n";
        assert_eq!(extract_sse_text(sse), "Hello");
    }

    #[test]
    fn extracts_chat_completion_json() {
        let json = r#"{"choices":[{"message":{"content":"hi there"}}]}"#;
        assert_eq!(extract_json_text(json), Some("hi there".to_string()));
    }

    #[test]
    fn extracts_completion_text_field() {
        let json = r#"{"choices":[{"text":"hi there"}]}"#;
        assert_eq!(extract_json_text(json), Some("hi there".to_string()));
    }

    #[test]
    fn falls_back_to_raw_for_unknown_shape() {
        let raw = b"not json at all";
        assert_eq!(extract_response_text("text/plain", raw), "not json at all");
    }

    #[tokio::test]
    async fn read_tail_entries_returns_empty_after_file_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("requests.jsonl");
        tokio::fs::write(&path, "{\"id\":0}\n").await.unwrap();
        tokio::fs::remove_file(&path).await.unwrap();

        let entries = read_tail_entries(&path, 10).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn read_tail_entries_returns_empty_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.jsonl");
        let entries = read_tail_entries(&path, 10).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn read_tail_entries_respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("requests.jsonl");
        let lines: Vec<String> = (0..5).map(|i| format!(r#"{{"id":{i}}}"#)).collect();
        tokio::fs::write(&path, lines.join("\n") + "\n")
            .await
            .unwrap();

        let entries = read_tail_entries(&path, 2).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["id"], 3);
        assert_eq!(entries[1]["id"], 4);
    }

    #[tokio::test]
    async fn end_to_end_proxy_forwards_and_logs() {
        let mut upstream = mockito::Server::new_async().await;
        let _m = upstream
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[{"message":{"content":"hi there"}}]}"#)
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            upstream_base_url: upstream.url(),
            port: 0,
            api_key: None,
            data_dir: dir.path().to_path_buf(),
        };
        let (_port, listener, router, _state) = bind(&config).await.unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });

        let client = reqwest::Client::new();
        let bound_port = _port;
        let resp = client
            .post(format!("http://127.0.0.1:{bound_port}/v1/chat/completions"))
            .body(r#"{"model":"gemma","messages":[{"role":"user","content":"hi"}]}"#)
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let text = resp.text().await.unwrap();
        assert!(text.contains("hi there"));

        server.abort();
    }
}
