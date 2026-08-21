//! Ланцюжки (chains) LLM-викликів: читання глобального trace для аналітики
//! + збереження chain-аналізу (вкладка «Ланцюжки»).
//!
//! Клієнт шле заголовки `x-chain-id`/`x-chain-step`/`x-chain-kind`/
//! `x-chain-cwd` з кожним локальним викликом і пише події ланцюжків у
//! глобальний trace (per-call записи з `chainId` + фінальний запис
//! `kind:"chain"`). Команди тут читають лише цей trace і opt-in body-capture
//! стор — жодної залежності від локального проксі (кореляція заголовків/
//! prompt_hash живе в окремому headless `myllm-proxy-service`,
//! `proxy-service/src/chain_correlation.rs`).
//!
//! # Два стори trace, не один
//!
//! Писемників історично два, і пишуть вони в РІЗНІ місця:
//!
//! ```text
//! ~/.n-cursor/llm-trace.jsonl              — JS-клієнт @7n/llm-lib: ОДИН файл
//! ~/.n-llm-lib/llm-trace-YYYY-MM-DD.jsonl  — Rust-крейт n7n-trace: ДЕННА ротація
//! ```
//!
//! Rust-порт конвеєрів (`n7n-llm-lib`/`n7n-harness` поверх `n7n-trace`) пише
//! в другий: денні файли в каталозі під власною env-змінною
//! (`N_LLM_TRACE_DIR`), із семиденним ретеншном на боці писемника. Доти цей
//! модуль знав лише перший шлях — тобто вкладка «Ланцюжки» не бачила ЖОДНОГО
//! рядка, написаного Rust-крейтами, включно з фінальними `kind:"chain"`.
//!
//! Тепер читаються ОБИДВА стори й зливаються в один хронологічний потік
//! (старіші перші — той самий порядок, що очікують `chains_list`/
//! `chain_steps`). Явний `N_LLM_TRACE_PATH`/`N_CURSOR_TRACE_PATH` лишається
//! перевизначенням на ОДИН файл і вимикає збір: хто задав шлях явно, той і
//! отримує рівно його.

use serde_json::{json, Value};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Ліміт хвоста trace-файла (запобіжник проти багаторічного файла).
const TRACE_TAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Явне перевизначення шляху на ОДИН файл, якщо задане. Вимикає збір із
/// двох сторів (док-шапка модуля): хто вказав файл явно, той отримує рівно
/// його.
fn explicit_trace_path() -> Option<PathBuf> {
    for var in ["N_LLM_TRACE_PATH", "N_CURSOR_TRACE_PATH"] {
        if let Ok(p) = std::env::var(var) {
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

/// Legacy-стор JS-клієнта `@7n/llm-lib`: один файл `~/.n-cursor/llm-trace.jsonl`.
fn legacy_trace_path(app: &tauri::AppHandle) -> PathBuf {
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    home.join(".n-cursor").join("llm-trace.jsonl")
}

/// Корінь стору Rust-крейта `n7n-trace` — каталог денних файлів. Той самий
/// резолв, що в самому крейті: `N_LLM_TRACE_DIR`, інакше `~/.n-llm-lib`.
fn trace_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("N_LLM_TRACE_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    home.join(".n-llm-lib")
}

/// Ім'я файлу → дата (`YYYY-MM-DD`), якщо воно ТОЧНО відповідає формі
/// `llm-trace-YYYY-MM-DD.jsonl`. Строга перевірка — щоб у джерела не
/// потрапляли сторонні файли, які просто лежать у тому самому каталозі
/// (дзеркалить `parse_trace_file_date` писемника).
fn daily_file_date(name: &str) -> Option<&str> {
    let date = name.strip_prefix("llm-trace-")?.strip_suffix(".jsonl")?;
    let bytes = date.as_bytes();
    let ok = date.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    ok.then_some(date)
}

/// Денні файли стору `n7n-trace`, ХРОНОЛОГІЧНО (старіші перші). Імена
/// `YYYY-MM-DD` сортуються лексикографічно = хронологічно, тож `stat` кожного
/// файлу не потрібен. Відсутній каталог → порожньо (Rust-крейти на цій машині
/// ще нічого не писали — не помилка).
async fn daily_trace_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(mut entries) = fs::read_dir(dir).await else {
        return Vec::new();
    };
    let mut dated: Vec<(String, PathBuf)> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(date) = daily_file_date(name) {
            dated.push((date.to_string(), path.clone()));
        }
    }
    dated.sort_by(|a, b| a.0.cmp(&b.0));
    dated.into_iter().map(|(_, p)| p).collect()
}

/// Усі джерела trace, ХРОНОЛОГІЧНО (старіші перші): спершу legacy-файл
/// JS-клієнта, далі денні файли `n7n-trace`.
///
/// Порядок «legacy → денні» не довільний: JS-пакет видалений, тобто його файл
/// історичний і цілком передує будь-якому Rust-запису. Точнішого злиття (за
/// `ts` кожного рядка) тут навмисно немає — воно коштувало б сортування всього
/// потоку заради порядку МІЖ сторами, який і так однозначний.
async fn trace_sources(app: &tauri::AppHandle) -> Vec<PathBuf> {
    if let Some(explicit) = explicit_trace_path() {
        return vec![explicit];
    }
    let mut sources = Vec::new();
    let legacy = legacy_trace_path(app);
    if fs::metadata(&legacy).await.is_ok() {
        sources.push(legacy);
    }
    sources.extend(daily_trace_files(&trace_dir(app)).await);
    sources
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

/// Читає хвости ВСІХ джерел під СПІЛЬНИМ байтовим бюджетом і зливає в один
/// хронологічний потік (старіші перші).
///
/// Бюджет витрачається від НОВІШИХ джерел до старіших: коли його не стає,
/// обрізаються найдавніші записи, а не найсвіжіші — вкладка показує останні
/// ланцюжки, тож саме вони мусять пережити кеп. Результат усе одно
/// повертається в хронологічному порядку.
async fn read_all_trace_sources(sources: &[PathBuf], budget: u64) -> Vec<Value> {
    let mut chunks: Vec<Vec<Value>> = Vec::with_capacity(sources.len());
    let mut left = budget;
    for path in sources.iter().rev() {
        if left == 0 {
            break;
        }
        let chunk = read_trace_tail(path, left).await;
        let spent = fs::metadata(path)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
            .min(left);
        left -= spent;
        chunks.push(chunk);
    }
    chunks.reverse();
    chunks.into_iter().flatten().collect()
}

/// Останні ланцюжки (фінальні записи `kind:"chain"`), новіші в кінці.
#[tauri::command]
pub async fn chains_list(
    app: tauri::AppHandle,
    limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    let records = read_all_trace_sources(&trace_sources(&app).await, TRACE_TAIL_BYTES).await;
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
    let records = read_all_trace_sources(&trace_sources(&app).await, TRACE_TAIL_BYTES).await;
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
/// Первинне джерело для chain-аналізу: на відміну від колишнього
/// `requests.jsonl` проксі (лише local), тут є й CLOUD-кроки.
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
pub async fn read_body_capture(
    app: tauri::AppHandle,
    chain_id: String,
) -> Result<Vec<Value>, String> {
    Ok(read_body_capture_dir(&bodies_dir(&app).join(&chain_id)).await)
}

// ─── Очистка trace (кнопка «Очистити» вкладки «Ланцюжки») ───

/// Трункейтить УСІ trace-файли (обидва стори, док-шапка модуля) і видаляє
/// body-capture стор. Відсутні шляхи — не помилка (trace ще не писався).
/// Файли саме трункейтяться, а не видаляються: у них паралельно дописують
/// живі клієнти, і забраний з-під них inode означав би тихо загублені записи
/// до кінця життя їхнього дескриптора.
async fn clear_trace_files(traces: &[PathBuf], bodies: &Path) -> Result<(), String> {
    for trace in traces {
        match fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(trace)
            .await
        {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    match fs::remove_dir_all(bodies).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Очищає дані вкладки «Ланцюжки»: `llm-trace.jsonl` + body-capture стор.
/// Збережені аналізи (insights + індекс) не чіпає.
#[tauri::command]
pub async fn chains_clear_trace(app: tauri::AppHandle) -> Result<(), String> {
    clear_trace_files(&trace_sources(&app).await, &bodies_dir(&app)).await
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

/// Unix-час у мс без залежності від chrono.
fn chrono_like_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn clear_trace_files_truncates_trace_and_removes_bodies() {
        let dir = tempfile::tempdir().unwrap();
        let trace = dir.path().join("trace.jsonl");
        let bodies = dir.path().join("bodies");
        std::fs::write(&trace, "{\"kind\":\"chain\"}\n").unwrap();
        std::fs::create_dir_all(bodies.join("c1")).unwrap();
        std::fs::write(bodies.join("c1").join("1.json"), "{}").unwrap();

        clear_trace_files(std::slice::from_ref(&trace), &bodies)
            .await
            .unwrap();

        // Файл лишився (у нього дописують клієнти), але порожній; стор тіл зник.
        assert_eq!(std::fs::metadata(&trace).unwrap().len(), 0);
        assert!(!bodies.exists());
    }

    #[tokio::test]
    async fn clear_trace_files_missing_paths_ok() {
        let dir = tempfile::tempdir().unwrap();
        clear_trace_files(
            &[dir.path().join("немає.jsonl")],
            &dir.path().join("немає-dir"),
        )
        .await
        .unwrap();
    }

    // ─── Два стори trace: денні файли n7n-trace + legacy-файл ───

    #[test]
    fn daily_file_date_accepts_only_the_exact_writer_shape() {
        assert_eq!(
            daily_file_date("llm-trace-2026-08-20.jsonl"),
            Some("2026-08-20")
        );
        // Стороннє в тому самому каталозі не мусить потрапляти в джерела.
        assert_eq!(daily_file_date("llm-trace.jsonl"), None);
        assert_eq!(daily_file_date("llm-trace-2026-08-20.json"), None);
        assert_eq!(daily_file_date("llm-trace-20260820.jsonl"), None);
        assert_eq!(daily_file_date("llm-trace-2026-08-2X.jsonl"), None);
        assert_eq!(daily_file_date("calibration.json"), None);
    }

    #[tokio::test]
    async fn daily_trace_files_are_chronological_and_skip_foreign_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "llm-trace-2026-08-20.jsonl",
            "llm-trace-2026-08-18.jsonl",
            "llm-trace-2026-08-19.jsonl",
            "not-a-trace.txt",
            "llm-trace.jsonl",
        ] {
            std::fs::write(dir.path().join(name), "{}\n").unwrap();
        }
        let files = daily_trace_files(dir.path()).await;
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![
                "llm-trace-2026-08-18.jsonl",
                "llm-trace-2026-08-19.jsonl",
                "llm-trace-2026-08-20.jsonl",
            ]
        );
    }

    #[tokio::test]
    async fn daily_trace_files_missing_dir_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(daily_trace_files(&dir.path().join("no-such"))
            .await
            .is_empty());
    }

    /// Ключовий тест зв'язки: рядки Rust-крейта `n7n-trace` (денні файли)
    /// мусять доїжджати до вкладки — доти цей модуль знав лише legacy-шлях і
    /// не бачив їх узагалі.
    #[tokio::test]
    async fn reads_both_stores_in_chronological_order() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("llm-trace.jsonl");
        std::fs::write(
            &legacy,
            "{\"kind\":\"one-shot\",\"chainId\":\"js\",\"marker\":\"legacy\"}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("llm-trace-2026-08-19.jsonl"),
            "{\"kind\":\"fix\",\"chainId\":\"rs\",\"marker\":\"day-19\"}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("llm-trace-2026-08-20.jsonl"),
            "{\"kind\":\"chain\",\"chainId\":\"rs\",\"marker\":\"day-20\"}\n",
        )
        .unwrap();

        let sources = {
            let mut v = vec![legacy];
            v.extend(daily_trace_files(dir.path()).await);
            v
        };
        let records = read_all_trace_sources(&sources, TRACE_TAIL_BYTES).await;
        let markers: Vec<&str> = records
            .iter()
            .filter_map(|r| r.get("marker").and_then(Value::as_str))
            .collect();
        assert_eq!(markers, vec!["legacy", "day-19", "day-20"]);
        // Фінальний запис Rust-крейта видно як ланцюжок.
        assert_eq!(
            records
                .iter()
                .filter(|r| r.get("kind").and_then(Value::as_str) == Some("chain"))
                .count(),
            1
        );
    }

    /// Бюджет витрачається від новіших джерел до старіших: під кепом
    /// виживають СВІЖІ записи, бо саме їх показує вкладка.
    #[tokio::test]
    async fn budget_drops_oldest_sources_first() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("llm-trace-2026-08-01.jsonl");
        let new = dir.path().join("llm-trace-2026-08-02.jsonl");
        // Кожен файл ~200 байт; бюджету вистачає лише на новіший.
        let filler = "x".repeat(150);
        std::fs::write(
            &old,
            format!("{{\"marker\":\"old\",\"pad\":\"{filler}\"}}\n"),
        )
        .unwrap();
        std::fs::write(
            &new,
            format!("{{\"marker\":\"new\",\"pad\":\"{filler}\"}}\n"),
        )
        .unwrap();

        let records = read_all_trace_sources(&[old, new], 200).await;
        let markers: Vec<&str> = records
            .iter()
            .filter_map(|r| r.get("marker").and_then(Value::as_str))
            .collect();
        assert_eq!(markers, vec!["new"], "під кепом виживає новіше джерело");
    }

    #[tokio::test]
    async fn clear_trace_files_truncates_every_source() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("llm-trace.jsonl");
        let daily = dir.path().join("llm-trace-2026-08-20.jsonl");
        let bodies = dir.path().join("bodies");
        std::fs::write(&legacy, "{\"kind\":\"chain\"}\n").unwrap();
        std::fs::write(&daily, "{\"kind\":\"chain\"}\n").unwrap();
        std::fs::create_dir_all(&bodies).unwrap();

        clear_trace_files(&[legacy.clone(), daily.clone()], &bodies)
            .await
            .unwrap();

        assert_eq!(std::fs::metadata(&legacy).unwrap().len(), 0);
        assert_eq!(
            std::fs::metadata(&daily).unwrap().len(),
            0,
            "денний файл теж"
        );
        assert!(!bodies.exists());
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
        std::fs::write(
            chain_dir.join("2.json"),
            r#"{"chainStep":2,"prompt":"друге"}"#,
        )
        .unwrap();
        std::fs::write(
            chain_dir.join("1.json"),
            r#"{"chainStep":1,"prompt":"перше"}"#,
        )
        .unwrap();
        std::fs::write(chain_dir.join("garbage.json"), "не json").unwrap();
        let bodies = read_body_capture_dir(&chain_dir).await;
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0]["prompt"], "перше");
        assert_eq!(bodies[1]["prompt"], "друге");
    }
}
