//! Кореляція проксі-запитів з ланцюжками `@7n/llm-lib`: `x-chain-*` заголовки
//! та fallback `prompt_hash`, записуються в `RequestLogEntry` (`proxy.rs`) для
//! подальшого джойну з глобальним trace-файлом `~/.n-cursor/llm-trace.jsonl`
//! (читає його myllm — вкладка «Ланцюжки», не цей сервіс).
//!
//! КОНТРАКТ `prompt_hash` (дзеркало `llm-lib/lib/chain.mjs::promptHash` — не
//! міняти односторонньо): sha256(trim(text)), перші 16 hex lowercase, де text —
//! content ОСТАННЬОГО повідомлення з role=="user"; рядковий content береться як
//! є, масив parts — конкатенація `part.text` для `part.type=="text"`. Хеш
//! рахується з ОРИГІНАЛЬНОГО тіла (до компресії) — клієнт хешує те, що надіслав.

use axum::http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use serde_json::json;

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

    #[test]
    fn percent_decode_roundtrip() {
        assert_eq!(percent_decode("%2Ftmp%2Fx"), "/tmp/x");
        assert_eq!(percent_decode("plain"), "plain");
    }
}
