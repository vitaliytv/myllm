//! Ланцюжки (chains) LLM-викликів: кореляція запитів проксі з ланцюжками
//! `@nitra/llm-lib` та читання глобального trace для аналітики.
//!
//! Клієнт (llm-lib) шле заголовки `x-chain-id`/`x-chain-step`/`x-chain-kind`/
//! `x-chain-cwd` з кожним локальним викликом і пише події ланцюжків у
//! `~/.n-cursor/llm-trace.jsonl` (per-call записи з `chainId` + фінальний
//! запис `kind:"chain"`). Проксі зберігає кореляційні поля у своєму лозі,
//! а команди `chains_list`/`chain_steps` читають trace для вкладки «Ланцюжки».
//!
//! КОНТРАКТ `prompt_hash` (дзеркало `llm-lib/lib/chain.mjs::promptHash` — не
//! міняти односторонньо): sha256(trim(text)), перші 16 hex lowercase, де text —
//! content ОСТАННЬОГО повідомлення з role=="user"; рядковий content береться як
//! є, масив parts — конкатенація `part.text` для `part.type=="text"`. Хеш
//! рахується з ОРИГІНАЛЬНОГО тіла (до компресії) — клієнт хешує те, що надіслав.

use axum::http::HeaderMap;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Ліміт хвоста trace-файла (запобіжник проти багаторічного файла).
const TRACE_TAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Кореляційні поля запиту, витягнуті із заголовків + fallback-хеш промпту.
#[derive(Debug, Default)]
pub struct ChainCorrelation {
    /// `x-chain-id` — id ланцюжка llm-lib (hex16).
    pub correlation_id: Option<String>,
    /// `x-chain-step` — номер кроку в ланцюжку.
    pub chain_step: Option<u32>,
    /// `x-chain-kind` — тип задачі (fix-concern, doc-generate, ...).
    pub chain_kind: Option<String>,
    /// `x-chain-cwd` — директорія виклику (urlencoded у заголовку).
    pub chain_cwd: Option<String>,
    /// Fallback-джойн: sha256 hex16 останнього user-повідомлення.
    pub prompt_hash: Option<String>,
}

/// Мінімальний percent-decode для `x-chain-cwd` (клієнт кодує через
/// `encodeURIComponent`; шляхи містять здебільшого `%2F`).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Текст повідомлення за контрактом: рядок як є; масив parts — конкатенація
/// `part.text` для `part.type == "text"`.
fn message_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let mut out = String::new();
            for p in parts {
                if p.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = p.get("text").and_then(Value::as_str) {
                        out.push_str(t);
                    }
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Хеш промпта за спільним контрактом (див. шапку модуля).
pub fn prompt_hash(request_body: &Value) -> Option<String> {
    let messages = request_body.get("messages")?.as_array()?;
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))?;
    let text = message_text(last_user.get("content")?)?;
    let digest = Sha256::digest(text.trim().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Some(hex[..16].to_string())
}

/// Витягує кореляційні поля із заголовків запиту + рахує prompt_hash з тіла.
pub fn extract_correlation(headers: &HeaderMap, request_body: Option<&Value>) -> ChainCorrelation {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    ChainCorrelation {
        correlation_id: get("x-chain-id"),
        chain_step: get("x-chain-step").and_then(|s| s.parse::<u32>().ok()),
        chain_kind: get("x-chain-kind"),
        chain_cwd: get("x-chain-cwd").map(|s| percent_decode(&s)),
        prompt_hash: request_body.and_then(prompt_hash),
    }
}

/// Шлях глобального trace llm-lib (той самий резолв, що `tracePath()` пакета).
fn trace_path(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("N_LLM_TRACE_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("N_CURSOR_TRACE_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    home.join(".n-cursor").join("llm-trace.jsonl")
}

/// Читає хвіст JSONL-файла (до `tail_bytes`), пропускаючи сміттєві рядки.
/// Відсутній файл → порожній список (trace ще не писався / інша машина).
async fn read_trace_tail(path: &Path, tail_bytes: u64) -> Vec<Value> {
    let Ok(mut file) = fs::File::open(path).await else {
        return Vec::new();
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(tail_bytes);
    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).await.is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    if start > 0 {
        // Перший рядок після seek потенційно обрізаний — відкидаємо.
        lines.next();
    }
    lines
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .collect()
}

/// Останні ланцюжки (фінальні записи `kind:"chain"`), новіші в кінці.
#[tauri::command]
pub async fn chains_list(
    app: tauri::AppHandle,
    limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    let records = read_trace_tail(&trace_path(&app), TRACE_TAIL_BYTES).await;
    let mut chains: Vec<Value> = records
        .into_iter()
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("chain"))
        .collect();
    let limit = limit.unwrap_or(200);
    if chains.len() > limit {
        chains.drain(..chains.len() - limit);
    }
    Ok(chains)
}

/// Кроки одного ланцюжка (per-call записи з цим `chainId`), у порядку файла.
#[tauri::command]
pub async fn chain_steps(app: tauri::AppHandle, chain_id: String) -> Result<Vec<Value>, String> {
    let records = read_trace_tail(&trace_path(&app), TRACE_TAIL_BYTES).await;
    Ok(records
        .into_iter()
        .filter(|r| {
            r.get("chainId").and_then(Value::as_str) == Some(chain_id.as_str())
                && r.get("kind").and_then(Value::as_str) != Some("chain")
        })
        .collect())
}

// ─── Читання body-capture (llm-lib opt-in, ~/.n-cursor/llm-bodies/) ───

/// Корінь body-capture стору (той самий резолв, що `bodiesDir()` пакета) —
/// повні тіла prompt/response, opt-in (`N_LLM_TRACE_BODIES=1` на клієнті).
/// Первинне джерело для chain-аналізу в direct-режимі (без myllm-проксі):
/// на відміну від `requests.jsonl` (лише local), тут є й CLOUD-кроки.
fn bodies_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("N_LLM_BODIES_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    home.join(".n-cursor").join("llm-bodies")
}

/// Читає всі `*.json`-тіла з `dir` (одна group-директорія body-capture стору),
/// сортує за `chainStep`. Відсутня/непридатна для читання директорія → порожній
/// список (body-capture не увімкнено для цього прогону — не помилка).
async fn read_body_capture_dir(dir: &Path) -> Vec<Value> {
    let Ok(mut entries) = fs::read_dir(dir).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(text) = fs::read_to_string(entry.path()).await else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            out.push(value);
        }
    }
    out.sort_by_key(|v| v.get("chainStep").and_then(Value::as_u64).unwrap_or(0));
    out
}

/// Повні тіла (prompt+response) кроків одного ланцюжка з body-capture стору
/// (`<bodiesDir>/<chainId>/*.json`). Порожній список — body-capture не
/// увімкнено для цього прогону (не помилка).
#[tauri::command]
pub async fn read_body_capture(app: tauri::AppHandle, chain_id: String) -> Result<Vec<Value>, String> {
    Ok(read_body_capture_dir(&bodies_dir(&app).join(&chain_id)).await)
}

// ─── Збереження результатів chain-аналізу (вкладка «Ланцюжки» → «Аналіз») ───

/// Корінь insights-стору: `~/.n-cursor/insights/` — глобальний cross-project
/// (поряд із telemetry; артефакти дистиляційного маховика).
fn insights_dir(app: &tauri::AppHandle) -> PathBuf {
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    home.join(".n-cursor").join("insights")
}

/// Шлях індексу аналізів у app-data (для UI-бейджів).
fn analyses_index_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("chain-analyses.jsonl"))
}

/// Зберігає markdown-результат аналізу ланцюжка у
/// `~/.n-cursor/insights/<kind>/<chainId>.md` (frontmatter + текст) і додає
/// запис в індекс `chain-analyses.jsonl`. Повертає шлях md-файла.
#[allow(clippy::too_many_arguments)] // Tauri-команда: поля приходять окремими JS-аргументами
#[tauri::command]
pub async fn save_chain_analysis(
    app: tauri::AppHandle,
    chain_id: String,
    chain_kind: String,
    unit: Option<String>,
    cwd: Option<String>,
    target_repo: Option<String>,
    model: Option<String>,
    markdown: String,
) -> Result<String, String> {
    // Санітизація компонентів шляху: id/kind ідуть у файлову систему.
    let safe = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    let dir = insights_dir(&app).join(safe(&chain_kind));
    fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.md", safe(&chain_id)));

    let ts = chrono_like_now_ms();
    let fm = format!(
        "---\nchainId: {chain_id}\nkind: {chain_kind}\nunit: {}\ncwd: {}\ntargetRepo: {}\nmodel: {}\nts: {ts}\n---\n\n",
        unit.as_deref().unwrap_or(""),
        cwd.as_deref().unwrap_or(""),
        target_repo.as_deref().unwrap_or(""),
        model.as_deref().unwrap_or("")
    );
    fs::write(&path, format!("{fm}{markdown}"))
        .await
        .map_err(|e| e.to_string())?;

    let index_path = analyses_index_path(&app)?;
    if let Some(parent) = index_path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let summary: String = markdown.chars().take(200).collect();
    let record = json!({
        "chainId": chain_id,
        "kind": chain_kind,
        "unit": unit,
        "cwd": cwd,
        "targetRepo": target_repo,
        "model": model,
        "path": path.to_string_lossy(),
        "ts": ts,
        "summary": summary,
    });
    let line = format!("{record}\n");
    let existing = fs::read_to_string(&index_path).await.unwrap_or_default();
    fs::write(&index_path, format!("{existing}{line}"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

/// Індекс збережених аналізів (для бейджів/перегляду в UI).
#[tauri::command]
pub async fn list_chain_analyses(app: tauri::AppHandle) -> Result<Vec<Value>, String> {
    let index_path = analyses_index_path(&app)?;
    let text = fs::read_to_string(&index_path).await.unwrap_or_default();
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .collect())
}

/// Unix-час у мс без залежності від chrono (стиль proxy.rs).
fn chrono_like_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn prompt_hash_string_content_deterministic() {
        let body = json!({"messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "  hello \n"}
        ]});
        // Дзеркальний вектор llm-lib chain.test.mjs: sha256("hello")[..16]
        assert_eq!(prompt_hash(&body).as_deref(), Some("2cf24dba5fb0a30e"));
        assert_eq!(prompt_hash(&body), prompt_hash(&body));
    }

    #[test]
    fn prompt_hash_parts_and_last_user() {
        let body = json!({"messages": [
            {"role": "user", "content": "перше"},
            {"role": "assistant", "content": "відповідь"},
            {"role": "user", "content": [
                {"type": "text", "text": "hel"},
                {"type": "image", "url": "x"},
                {"type": "text", "text": "lo"}
            ]}
        ]});
        assert_eq!(prompt_hash(&body).as_deref(), Some("2cf24dba5fb0a30e"));
    }

    #[test]
    fn prompt_hash_none_without_messages() {
        assert_eq!(prompt_hash(&json!({"model": "x"})), None);
        assert_eq!(prompt_hash(&json!({"messages": []})), None);
    }

    #[test]
    fn extract_correlation_full_and_empty() {
        let mut headers = HeaderMap::new();
        headers.insert("x-chain-id", HeaderValue::from_static("abc123"));
        headers.insert("x-chain-step", HeaderValue::from_static("3"));
        headers.insert("x-chain-kind", HeaderValue::from_static("fix-concern"));
        headers.insert(
            "x-chain-cwd",
            HeaderValue::from_static("%2FUsers%2Fx%2Fproj"),
        );
        let body = json!({"messages": [{"role": "user", "content": "hello"}]});
        let c = extract_correlation(&headers, Some(&body));
        assert_eq!(c.correlation_id.as_deref(), Some("abc123"));
        assert_eq!(c.chain_step, Some(3));
        assert_eq!(c.chain_kind.as_deref(), Some("fix-concern"));
        assert_eq!(c.chain_cwd.as_deref(), Some("/Users/x/proj"));
        assert_eq!(c.prompt_hash.as_deref(), Some("2cf24dba5fb0a30e"));

        let empty = extract_correlation(&HeaderMap::new(), None);
        assert!(empty.correlation_id.is_none());
        assert!(empty.prompt_hash.is_none());
    }

    #[test]
    fn extract_correlation_bad_step_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert("x-chain-step", HeaderValue::from_static("not-a-number"));
        let c = extract_correlation(&headers, None);
        assert_eq!(c.chain_step, None);
    }

    #[tokio::test]
    async fn read_trace_tail_filters_garbage_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        let missing = read_trace_tail(&path, TRACE_TAIL_BYTES).await;
        assert!(missing.is_empty());

        let content = concat!(
            "{\"kind\":\"one-shot\",\"chainId\":\"c1\",\"chainStep\":1}\n",
            "не json\n",
            "{\"kind\":\"chain\",\"chainId\":\"c1\",\"outcome\":\"success\",\"невідоме\":true}\n",
            "{\"обірваний"
        );
        std::fs::write(&path, content).unwrap();
        let records = read_trace_tail(&path, TRACE_TAIL_BYTES).await;
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[1].get("kind").and_then(Value::as_str),
            Some("chain")
        );
    }

    #[tokio::test]
    async fn read_trace_tail_caps_and_drops_first_partial_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("{{\"kind\":\"one-shot\",\"i\":{i}}}\n"));
        }
        std::fs::write(&path, &content).unwrap();
        // Хвіст 1KB: перший (потенційно обрізаний) рядок відкинуто, парс не падає.
        let records = read_trace_tail(&path, 1024).await;
        assert!(!records.is_empty());
        assert!(records.len() < 1000);
    }

    #[test]
    fn percent_decode_roundtrip() {
        assert_eq!(percent_decode("%2Ftmp%2Fx"), "/tmp/x");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[tokio::test]
    async fn read_body_capture_dir_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = read_body_capture_dir(&dir.path().join("no-such-chain")).await;
        assert!(missing.is_empty());
    }

    #[tokio::test]
    async fn read_body_capture_dir_sorts_by_chain_step_and_skips_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let chain_dir = dir.path().join("c1");
        std::fs::create_dir_all(&chain_dir).unwrap();
        std::fs::write(chain_dir.join("2.json"), r#"{"chainStep":2,"prompt":"друге"}"#).unwrap();
        std::fs::write(chain_dir.join("1.json"), r#"{"chainStep":1,"prompt":"перше"}"#).unwrap();
        std::fs::write(chain_dir.join("garbage.json"), "не json").unwrap();
        let bodies = read_body_capture_dir(&chain_dir).await;
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0]["prompt"], "перше");
        assert_eq!(bodies[1]["prompt"], "друге");
    }
}
