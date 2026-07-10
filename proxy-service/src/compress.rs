//! Стиснення тіла chat-completions запиту перед форвардом на upstream:
//! minify вбудованого pretty-printed JSON у текстовому `content` + обрізання
//! старих великих блоків. Викликається з `proxy_handler` (`proxy.rs`), який
//! підміняє тіло запиту на стиснене, лишаючи оригінал у логі історії.
//!
//! Проксі generic для `/v1/*`. Запит зі structured `response_format`
//! пропускається цілком незмінним (стосується форми всієї відповіді).
//! Tool/function-повідомлення (`tool_calls`/`function_call`/`role: tool`)
//! лишаються byte-exact по-повідомленнєво — а не блокують компресію решти
//! розмови: емпірично саме довгі agent-сесії з tool calls (`mcp-omlx.mjs`)
//! ростуть до розмірів, що впираються в Memory Guard, тож all-or-nothing
//! skip на рівні цілого запиту залишав їх зовсім нестиснутими.
//!
//! `system`-повідомлення за замовчуванням теж захищене (там живуть
//! інструкції агента й каталог доступних skills — саме інформація, не
//! бойлерплейт, див. аналіз `requests.jsonl`), але цей захист знімається,
//! коли сумарний розмір запиту перевищує `SYSTEM_TRUNCATION_SIZE_THRESHOLD`:
//! у цій зоні запит і так ризикує впертись у `prefill_memory_exceeded`, тож
//! часткова втрата каталогу skills — менша шкода за повну відмову prefill.
//!
//! КАНОНІЧНА логіка компресії (spec 2026-07-06-proxy-retirement) перенесена
//! на клієнт — `@7n/llm-lib/lib/internal/compress-context.mjs` (той самий
//! алгоритм, адаптований під форму pi Context замість OpenAI-body), wired
//! у кожен раннер пакета через streamFn-mixin `apply-compression.mjs`. Це
//! Rust-копія відповідає лише за проксі-шлях (клієнти БЕЗ llm-lib, що досі
//! ходять через myllm-проксі як debug/legacy-тул) — не єдине джерело правди.

use serde_json::Value;

/// Скільки останніх повідомлень лишаємо повністю захищеними від truncation
/// (тільки безпечний minify) — вони найбільше впливають на якість відповіді.
const PROTECTED_TAIL_MESSAGES: usize = 2;
/// Поріг розміру `content` (у символах), з якого блок — кандидат на обрізання.
const TRUNCATE_THRESHOLD: usize = 4000;
const TRUNCATE_HEAD: usize = 1500;
const TRUNCATE_TAIL: usize = 500;
/// Мінімальна довжина вбудованого JSON-блоку, щоб турбуватись мінізацією.
const MIN_JSON_MINIFY_LEN: usize = 40;
/// Поріг сумарного розміру сирого тіла запиту (байти "на дроті", до
/// парсингу), понад який `system`-повідомлення перестає бути захищеним від
/// truncation. Емпіричний аналіз реальних `prefill_memory_exceeded` з
/// requests.jsonl показав, що вихід (успіх/відмова) НЕ визначається розміром
/// однозначно — той самий розмір 149719B і 206603B траплявся і в успішних, і
/// у провалених запитах (залежить від того, скільки Metal-пам'яті вже зайнято
/// в цю мить, Memory Guard). Найменший реальний провал — 149719B, тож поріг
/// навмисно нижчий з запасом, а не впритул до нього: мета — встигнути
/// зреагувати ще до входу в зону, де вже траплялись відмови, а не рівно на її
/// межі. Коли запит і так у цій ризиковій зоні, часткова втрата
/// `<available_skills>`-каталогу в системному промпті — менша шкода, ніж
/// повна відмова `prefill_memory_exceeded`.
const SYSTEM_TRUNCATION_SIZE_THRESHOLD: usize = 120_000;

/// Пробує стиснути тіло chat-completions запиту. `original_size` — розмір
/// сирого тіла запиту "на дроті" (до парсингу), потрібен лише для перемикання
/// захисту `system`-повідомлення. Повертає `None`, якщо запит не підпадає під
/// формат (немає `messages`), задає structured `response_format`, або
/// стиснення нічого не змінило.
pub fn compress_request_body(body: &Value, original_size: usize) -> Option<Value> {
    let messages = body.get("messages")?.as_array()?;
    if body.get("response_format").is_some() {
        return None;
    }

    let system_protected = original_size <= SYSTEM_TRUNCATION_SIZE_THRESHOLD;
    let last_unprotected = messages.len().saturating_sub(PROTECTED_TAIL_MESSAGES);
    let mut changed = false;
    let mut new_messages = Vec::with_capacity(messages.len());
    for (i, message) in messages.iter().enumerate() {
        if has_tool_payload(message) {
            new_messages.push(message.clone());
            continue;
        }
        let is_system = message.get("role").and_then(|r| r.as_str()) == Some("system");
        // Система має власне, незалежне від позиції правило (`system_protected`),
        // а не позиційний tail-захист — саме тому цільово обрізається навіть у
        // короткій 2-повідомленнєвій розмові, де вона інакше й так лишилась би
        // в захищеному хвості.
        let protected = if is_system {
            system_protected
        } else {
            i >= last_unprotected
        };
        match compress_message(message, protected) {
            Some(compressed) => {
                changed = true;
                new_messages.push(compressed);
            }
            None => new_messages.push(message.clone()),
        }
    }

    if !changed {
        return None;
    }
    let mut new_body = body.clone();
    new_body["messages"] = Value::Array(new_messages);
    Some(new_body)
}

/// `true`, коли повідомлення несе tool/function-виклик чи є результатом
/// такого виклику — його не можна ні мінізувати, ні обрізати: `content`
/// тут прив'язаний до exact-match аргументів чи виконаного інструменту.
fn has_tool_payload(message: &Value) -> bool {
    message.get("tool_calls").is_some()
        || message.get("function_call").is_some()
        || message.get("role").and_then(|r| r.as_str()) == Some("tool")
}

fn compress_message(message: &Value, protected: bool) -> Option<Value> {
    let content = message.get("content")?;
    let (new_content, changed) = match content {
        Value::String(s) => {
            let (text, c) = compress_text(s, protected);
            (Value::String(text), c)
        }
        Value::Array(parts) => compress_parts(parts, protected),
        _ => return None,
    };
    if !changed {
        return None;
    }
    let mut new_message = message.clone();
    new_message["content"] = new_content;
    Some(new_message)
}

/// Multi-modal `content` (масив частин `{type: "text", text: "..."}` /
/// `{type: "image_url", ...}`) — компресуємо лише текстові частини.
fn compress_parts(parts: &[Value], protected: bool) -> (Value, bool) {
    let mut changed = false;
    let mut new_parts = Vec::with_capacity(parts.len());
    for part in parts {
        let text = (part.get("type").and_then(|t| t.as_str()) == Some("text"))
            .then(|| part.get("text").and_then(|t| t.as_str()))
            .flatten();
        match text {
            Some(text) => {
                let (new_text, c) = compress_text(text, protected);
                if c {
                    changed = true;
                    let mut new_part = part.clone();
                    new_part["text"] = Value::String(new_text);
                    new_parts.push(new_part);
                } else {
                    new_parts.push(part.clone());
                }
            }
            None => new_parts.push(part.clone()),
        }
    }
    (Value::Array(new_parts), changed)
}

fn compress_text(text: &str, protected: bool) -> (String, bool) {
    let (minified, changed) = minify_embedded_json(text);
    if protected || minified.len() <= TRUNCATE_THRESHOLD {
        return (minified, changed);
    }
    (truncate_middle(&minified), true)
}

/// Ріже великий текст навпіл, лишаючи початок+кінець з міткою — символьно
/// безпечно (працює по `char`, не по байтах, щоб не розрізати UTF-8).
fn truncate_middle(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= TRUNCATE_HEAD + TRUNCATE_TAIL {
        return text.to_string();
    }
    let head: String = chars[..TRUNCATE_HEAD].iter().collect();
    let tail: String = chars[chars.len() - TRUNCATE_TAIL..].iter().collect();
    let dropped = chars.len() - TRUNCATE_HEAD - TRUNCATE_TAIL;
    format!("{head}\n...[truncated {dropped} chars]...\n{tail}")
}

/// Знаходить top-level JSON-значення, вбудовані у вільний текст (напр.
/// pretty-printed тіло HTTP-запиту в аналітичному промпті `pi`,
/// `pi-prompt.js:buildAnalysisPrompt`), і мінізує їх — прибирає
/// форматувальні пробіли без втрати даних. Решта тексту лишається незмінною.
fn minify_embedded_json(text: &str) -> (String, bool) {
    let mut result = String::with_capacity(text.len());
    let mut changed = false;
    let mut rest = text;
    while let Some(idx) = rest.find(['{', '[']) {
        result.push_str(&rest[..idx]);
        let candidate = &rest[idx..];
        let mut stream = serde_json::Deserializer::from_str(candidate).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                let consumed = stream.byte_offset();
                let raw = &candidate[..consumed];
                if raw.len() >= MIN_JSON_MINIFY_LEN && raw.contains('\n') {
                    if let Ok(minified) = serde_json::to_string(&value) {
                        if minified.len() < raw.len() {
                            result.push_str(&minified);
                            changed = true;
                            rest = &candidate[consumed..];
                            continue;
                        }
                    }
                }
                result.push_str(raw);
                rest = &candidate[consumed..];
            }
            _ => {
                // Не валідний JSON з цієї позиції — лишаємо саму дужку як є
                // і рухаємось далі (обидва кандидати `{`/`[` — 1 байт ASCII).
                result.push_str(&candidate[..1]);
                rest = &candidate[1..];
            }
        }
    }
    result.push_str(rest);
    (result, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn minifies_embedded_pretty_json_block() {
        let text = "Запит:\n{\n  \"a\": 1,\n  \"b\": [1, 2, 3],\n  \"c\": \"hello world\"\n}\n\nдалі текст";
        let (out, changed) = minify_embedded_json(text);
        assert!(changed);
        assert!(out.contains(r#"{"a":1,"b":[1,2,3],"c":"hello world"}"#));
        assert!(out.starts_with("Запит:\n"));
        assert!(out.ends_with("далі текст"));
    }

    #[test]
    fn leaves_plain_text_without_json_unchanged() {
        let text = "звичайний текст без жодних дужок JSON";
        let (out, changed) = minify_embedded_json(text);
        assert!(!changed);
        assert_eq!(out, text);
    }

    #[test]
    fn ignores_invalid_json_looking_braces() {
        let text = "код: { not: valid, json here }";
        let (out, changed) = minify_embedded_json(text);
        assert!(!changed);
        assert_eq!(out, text);
    }

    #[test]
    fn skips_short_json_blocks_even_without_newlines() {
        let text = "ok: {\"a\":1}";
        let (out, changed) = minify_embedded_json(text);
        assert!(!changed);
        assert_eq!(out, text);
    }

    /// Будує 5-повідомленнєву розмову з одним tool-ish повідомленням
    /// (передається як `tool_message`) і великим старим текстовим блоком,
    /// що є кандидатом на truncation.
    fn conversation_with_tool_message(tool_message: Value) -> Value {
        let big_json = format!("{{\n  \"data\": \"{}\"\n}}", "x".repeat(6000));
        json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": big_json},
                tool_message,
                {"role": "assistant", "content": "остання відповідь"},
                {"role": "user", "content": "останнє питання"},
            ]
        })
    }

    #[test]
    fn leaves_tool_calls_message_untouched_but_compresses_others() {
        let tool_message = json!({
            "role": "assistant",
            "tool_calls": [{"id": "1"}],
            "content": "{\n  \"x\": 1\n}"
        });
        let body = conversation_with_tool_message(tool_message.clone());
        let compressed =
            compress_request_body(&body, 0).expect("should compress the non-tool message");
        let msgs = compressed["messages"].as_array().unwrap();
        assert_eq!(
            msgs[2], tool_message,
            "tool_calls message must stay byte-exact"
        );
        assert!(msgs[1]["content"].as_str().unwrap().contains("truncated"));
    }

    #[test]
    fn leaves_function_call_message_untouched_but_compresses_others() {
        let tool_message = json!({
            "role": "assistant",
            "function_call": {"name": "f"},
            "content": "{\n  \"x\": 1\n}"
        });
        let body = conversation_with_tool_message(tool_message.clone());
        let compressed =
            compress_request_body(&body, 0).expect("should compress the non-tool message");
        let msgs = compressed["messages"].as_array().unwrap();
        assert_eq!(
            msgs[2], tool_message,
            "function_call message must stay byte-exact"
        );
        assert!(msgs[1]["content"].as_str().unwrap().contains("truncated"));
    }

    #[test]
    fn leaves_tool_role_message_untouched_but_compresses_others() {
        let tool_message = json!({"role": "tool", "content": "{\n  \"x\": 1\n}"});
        let body = conversation_with_tool_message(tool_message.clone());
        let compressed =
            compress_request_body(&body, 0).expect("should compress the non-tool message");
        let msgs = compressed["messages"].as_array().unwrap();
        assert_eq!(
            msgs[2], tool_message,
            "role: tool message must stay byte-exact"
        );
        assert!(msgs[1]["content"].as_str().unwrap().contains("truncated"));
    }

    #[test]
    fn skips_requests_with_response_format() {
        let body = json!({
            "response_format": {"type": "json_schema"},
            "messages": [{"role": "user", "content": "{\n  \"x\": 1\n}"}]
        });
        assert!(compress_request_body(&body, 0).is_none());
    }

    #[test]
    fn skips_requests_without_messages_array() {
        let body = json!({"model": "gemma"});
        assert!(compress_request_body(&body, 0).is_none());
    }

    #[test]
    fn minifies_old_message_and_truncates_large_old_block() {
        let big_json = format!("{{\n  \"data\": \"{}\"\n}}", "x".repeat(6000));
        let body = json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": big_json},
                {"role": "assistant", "content": "остання відповідь"},
                {"role": "user", "content": "останнє питання"},
            ]
        });
        let compressed = compress_request_body(&body, 0).expect("should compress");
        let msgs = compressed["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["content"], "sys");
        let old_content = msgs[1]["content"].as_str().unwrap();
        assert!(old_content.contains("truncated"));
        assert!(old_content.len() < 6000);
        assert_eq!(msgs[2]["content"], "остання відповідь");
        assert_eq!(msgs[3]["content"], "останнє питання");
    }

    #[test]
    fn protects_last_messages_from_truncation_even_if_large() {
        let big_text = format!("{{\n  \"data\": \"{}\"\n}}", "y".repeat(6000));
        let body = json!({
            "messages": [{"role": "user", "content": big_text.clone()}],
        });
        let compressed = compress_request_body(&body, 0).expect("should minify");
        let content = compressed["messages"][0]["content"].as_str().unwrap();
        assert!(!content.contains("truncated"));
        assert!(content.len() < big_text.len());
    }

    /// Розмова з 2 повідомленнями (system+user) і великим system-контентом —
    /// імітує реальний `pi`-запит, де `<available_skills>`-каталог робить
    /// system message найбільшим блоком запиту.
    fn conversation_with_big_system(system_len: usize) -> Value {
        json!({
            "messages": [
                {"role": "system", "content": "x".repeat(system_len)},
                {"role": "user", "content": "останнє питання"},
            ]
        })
    }

    #[test]
    fn keeps_system_message_protected_when_request_under_size_threshold() {
        let body = conversation_with_big_system(6000);
        // Розмір тіла нижче порогу — system лишається захищеним від truncation.
        let compressed = compress_request_body(&body, SYSTEM_TRUNCATION_SIZE_THRESHOLD);
        assert!(
            compressed.is_none(),
            "нема JSON для minify і система захищена — нічого стискати"
        );
    }

    #[test]
    fn truncates_system_message_when_request_exceeds_size_threshold() {
        let body = conversation_with_big_system(6000);
        let over_threshold = SYSTEM_TRUNCATION_SIZE_THRESHOLD + 1;
        let compressed = compress_request_body(&body, over_threshold)
            .expect("system message should become truncatable over the threshold");
        let system_content = compressed["messages"][0]["content"].as_str().unwrap();
        assert!(system_content.contains("truncated"));
        assert!(system_content.len() < 6000);
        // Останнє (і єдине окрім system) повідомлення лишається недоторканим —
        // поріг зачіпає лише `system`, не tail-захист інших повідомлень.
        assert_eq!(compressed["messages"][1]["content"], "останнє питання");
    }

    #[test]
    fn compresses_multimodal_text_part_and_skips_image_part() {
        let big_json = format!("{{\n  \"data\": \"{}\"\n}}", "z".repeat(6000));
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": big_json},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}}
                ]},
                {"role": "assistant", "content": "ok"},
                {"role": "user", "content": "ще одне"},
            ]
        });
        let compressed = compress_request_body(&body, 0).expect("should compress");
        let parts = compressed["messages"][0]["content"].as_array().unwrap();
        let text = parts[0]["text"].as_str().unwrap();
        assert!(text.contains("truncated"));
        assert_eq!(parts[1]["type"], "image_url");
    }
}
