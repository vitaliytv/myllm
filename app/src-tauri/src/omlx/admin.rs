//! Клієнт до admin API реального omlx-сервера (`/admin/api/login` +
//! `/admin/api/stats` + `/admin/api/global-settings`).
//!
//! `/admin/api/stats` вимагає cookie-сесію, яку видає `/admin/api/login` за
//! API-ключем — тому тримаємо один `reqwest::Client` з увімкненим cookie
//! store на весь час підключення, а не окремий клієнт на кожен запит.
//! Схему відповіді свідомо не типізуємо жорстко (omlx — зовнішній проєкт,
//! що розвивається сам собою): повертаємо сирий `serde_json::Value`, форму
//! під UI будує JS-шар (`services/omlx-queue.js`).

use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdminError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("omlx login failed with HTTP {status}: {message}")]
    LoginFailed { status: u16, message: String },
    #[error("not connected — call omlx_connect first")]
    NotConnected,
}

impl From<AdminError> for String {
    fn from(value: AdminError) -> Self {
        value.to_string()
    }
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    api_key: &'a str,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub client: reqwest::Client,
    pub base_url: String,
}

#[derive(Default)]
pub struct OmlxState(pub Mutex<Option<Session>>);

async fn connect_inner(base_url: &str, api_key: &str) -> Result<Session, AdminError> {
    let client = reqwest::Client::builder().cookie_store(true).build()?;
    let resp = client
        .post(format!("{base_url}/admin/api/login"))
        .json(&LoginRequest { api_key })
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let message = resp.text().await.unwrap_or_default();
        return Err(AdminError::LoginFailed {
            status: status.as_u16(),
            message,
        });
    }
    Ok(Session {
        client,
        base_url: base_url.to_string(),
    })
}

async fn get_json(session: &Session, path: &str) -> Result<Value, AdminError> {
    let resp = session
        .client
        .get(format!("{}{path}", session.base_url))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    if !status.is_success() {
        return Err(AdminError::LoginFailed {
            status: status.as_u16(),
            message: body.to_string(),
        });
    }
    Ok(body)
}

/// API-ключ із оточення процесу (`OMLX_API_KEY`) — той самий env, яким
/// користується `~/.claude/mcp-omlx.mjs`. Працює при запуску з shell
/// (`bun run start`); GUI-запуск з Finder shell-env не успадковує — тоді
/// повертається None і UI чекає ручного вводу.
#[tauri::command]
pub fn omlx_env_api_key() -> Option<String> {
    std::env::var("OMLX_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
}

#[tauri::command]
pub async fn omlx_connect(
    base_url: String,
    api_key: String,
    state: tauri::State<'_, OmlxState>,
) -> Result<(), String> {
    let session = connect_inner(&base_url, &api_key).await?;
    *state.0.lock().unwrap() = Some(session);
    Ok(())
}

/// Клонує сесію з-під lock'а і одразу відпускає його — тримати `MutexGuard`
/// через `.await` заборонено (не `Send`, tauri-команди мають бути `Send`).
fn current_session(state: &tauri::State<'_, OmlxState>) -> Result<Session, AdminError> {
    state
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or(AdminError::NotConnected)
}

#[tauri::command]
pub async fn omlx_stats(state: tauri::State<'_, OmlxState>) -> Result<Value, String> {
    let session = current_session(&state)?;
    Ok(get_json(&session, "/admin/api/stats").await?)
}

#[tauri::command]
pub async fn omlx_global_settings(state: tauri::State<'_, OmlxState>) -> Result<Value, String> {
    let session = current_session(&state)?;
    Ok(get_json(&session, "/admin/api/global-settings").await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_stores_cookie_session() {
        let mut server = mockito::Server::new_async().await;
        let _login = server
            .mock("POST", "/admin/api/login")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({ "api_key": "secret" }),
            ))
            .with_status(200)
            .with_header("set-cookie", "session=abc; Path=/")
            .with_body(r#"{"success":true}"#)
            .create_async()
            .await;

        let session = connect_inner(&server.url(), "secret").await.unwrap();
        assert_eq!(session.base_url, server.url());
    }

    #[tokio::test]
    async fn connect_rejects_bad_key() {
        let mut server = mockito::Server::new_async().await;
        let _login = server
            .mock("POST", "/admin/api/login")
            .with_status(401)
            .with_body(r#"{"detail":"invalid api key"}"#)
            .create_async()
            .await;

        let err = connect_inner(&server.url(), "wrong").await.unwrap_err();
        match err {
            AdminError::LoginFailed { status, message } => {
                assert_eq!(status, 401);
                assert!(message.contains("invalid api key"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_json_returns_parsed_body() {
        let mut server = mockito::Server::new_async().await;
        let _stats = server
            .mock("GET", "/admin/api/stats")
            .with_status(200)
            .with_body(r#"{"total_requests":60}"#)
            .create_async()
            .await;

        let session = connect_inner_for_test(&server.url());
        let body = get_json(&session, "/admin/api/stats").await.unwrap();
        assert_eq!(body["total_requests"], 60);
    }

    fn connect_inner_for_test(base_url: &str) -> Session {
        Session {
            client: reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .unwrap(),
            base_url: base_url.to_string(),
        }
    }
}
