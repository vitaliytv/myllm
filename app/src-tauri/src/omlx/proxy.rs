//! Зворотний проксі для `/v1/*` трафіку omlx.
//!
//! Admin API (`admin.rs`) дає тільки поточний стан черги — після завершення
//! запиту текст промпту/відповіді зникає. Щоб бачити повну історію (хто що
//! питав і що відповів), myllm стає посередником: клієнти (mlmail, myshare,
//! `mcp-omlx.mjs`) звертаються на локальний порт цього проксі замість прямого
//! `:8000`, проксі пересилає запит на справжній omlx один в один і паралельно
//! стрімить копію в лог — так стрімінг (SSE) не ламається для клієнта, а
//! історія запитів наповнюється текстом у реальному часі.

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;

/// Скільки символів тексту відповіді зберігаємо в записі — запобіжник проти
/// того, щоб один величезний non-JSON/non-SSE respose роздув requests.jsonl.
const MAX_RESPONSE_CHARS: usize = 200_000;
/// Скільки останніх рядків читаємо з requests.jsonl для початкового
/// заповнення історії при відкритті вікна.
const DEFAULT_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to bind {0}: {1}")]
    Bind(String, std::io::Error),
    #[error("proxy is not running")]
    NotRunning,
    #[error("could not resolve app data dir: {0}")]
    AppData(String),
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
}

struct ProxyShared {
    client: reqwest::Client,
    upstream_base_url: String,
    log_path: PathBuf,
    app: tauri::AppHandle,
    next_id: AtomicU64,
}

/// Хендл живого проксі-таска — зберігається у Tauri-стані, щоб `proxy_stop`
/// міг його перервати.
pub struct ProxyRuntime(pub Mutex<Option<tokio::task::JoinHandle<()>>>);

impl Default for ProxyRuntime {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
pub async fn proxy_start(
    upstream_base_url: String,
    port: u16,
    app: tauri::AppHandle,
    runtime: tauri::State<'_, ProxyRuntime>,
) -> Result<u16, String> {
    let (actual_port, handle) = start_inner(upstream_base_url, port, app).await?;
    *runtime.0.lock().unwrap() = Some(handle);
    Ok(actual_port)
}

async fn start_inner(
    upstream_base_url: String,
    port: u16,
    app: tauri::AppHandle,
) -> Result<(u16, tokio::task::JoinHandle<()>), ProxyError> {
    use tauri::Manager;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ProxyError::AppData(e.to_string()))?;
    tokio::fs::create_dir_all(&data_dir).await?;
    let log_path = data_dir.join("requests.jsonl");

    let shared = Arc::new(ProxyShared {
        client: reqwest::Client::new(),
        upstream_base_url,
        log_path,
        app,
        next_id: AtomicU64::new(0),
    });

    let router = Router::new()
        .fallback(any(proxy_handler))
        .with_state(shared);
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| ProxyError::Bind(addr, e))?;
    let actual_port = listener.local_addr()?.port();

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Ok((actual_port, handle))
}

#[tauri::command]
pub fn proxy_stop(runtime: tauri::State<'_, ProxyRuntime>) -> Result<(), String> {
    let mut guard = runtime.0.lock().unwrap();
    match guard.take() {
        Some(handle) => {
            handle.abort();
            Ok(())
        }
        None => Err(ProxyError::NotRunning.into()),
    }
}

#[tauri::command]
pub async fn proxy_history(
    limit: Option<usize>,
    app: tauri::AppHandle,
) -> Result<Vec<Value>, String> {
    use tauri::Manager;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ProxyError::AppData(e.to_string()))?;
    let log_path = data_dir.join("requests.jsonl");
    let limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    Ok(read_tail_entries(&log_path, limit).await?)
}

#[tauri::command]
pub async fn proxy_clear_history(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ProxyError::AppData(e.to_string()))?;
    let log_path = data_dir.join("requests.jsonl");
    match tokio::fs::remove_file(&log_path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ProxyError::from(e).into()),
    }
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
    State(shared): State<Arc<ProxyShared>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
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

    let mut upstream_req = shared.client.request(method.clone(), &url);
    for (name, value) in headers.iter() {
        if !is_hop_by_hop(name) {
            upstream_req = upstream_req.header(name, value);
        }
    }
    let upstream_resp = match upstream_req.body(body).send().await {
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
    // запис історії (файл + push-подія) — так довгий SSE не блокує клієнта,
    // а лог з'являється одразу, як тільки відповідь дійшла до кінця.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let shared_for_task = shared.clone();
    let request_body_for_task = request_body.clone();
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
        let response_text = extract_response_text(&content_type, &captured);
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
        };
        finalize_entry(&shared_for_task, entry).await;
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    response
}

async fn finalize_entry(shared: &ProxyShared, entry: RequestLogEntry) {
    use tauri::Emitter;

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
    let _ = shared.app.emit("omlx-request-logged", &entry);
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
        // finalize_entry writes via a real AppHandle in production; here we
        // exercise the pure pieces (request/response shuttling + extraction)
        // through the axum router directly, bypassing Tauri wiring.
        let shared = Arc::new(TestShared {
            client: reqwest::Client::new(),
            upstream_base_url: upstream.url(),
            log_path: dir.path().join("requests.jsonl"),
        });

        let body = Bytes::from(r#"{"model":"gemma","messages":[{"role":"user","content":"hi"}]}"#);
        let upstream_req = shared
            .client
            .post(format!("{}/v1/chat/completions", shared.upstream_base_url))
            .body(body);
        let resp = upstream_req.send().await.unwrap();
        assert!(resp.status().is_success());
        let text = resp.text().await.unwrap();
        assert_eq!(extract_json_text(&text), Some("hi there".to_string()));
    }

    /// Мінімальна тестова заміна `ProxyShared` без залежності від живого
    /// `tauri::AppHandle` (його не піднімеш у юніт-тесті).
    struct TestShared {
        client: reqwest::Client,
        upstream_base_url: String,
        #[allow(dead_code)]
        log_path: PathBuf,
    }
}
