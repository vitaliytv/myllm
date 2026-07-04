//! Запуск зовнішнього кодинг-агента `pi` (https://pi.dev) у директорії
//! процесу-клієнта проксі-запиту — кнопка над записом історії в `App.vue`
//! відкриває випадаючий список моделей (з `N_*_MODEL` env-змінних) і чат-сесію,
//! кожна репліка якої йде окремим короткоживучим викликом
//! `pi --print --session-id <id> ...`; безперервність розмови тримає сам `pi`
//! через сесійний файл на диску (не RPC/довготривалий процес). `pi` отримує
//! повний read/bash/edit/write доступ (дефолт) — може сам вносити правки в код
//! проєкту за вказаним шляхом.

use serde::Serialize;
use tauri_plugin_shell::ShellExt;
use thiserror::Error;

// `pi`'s default system prompt + auto-loaded AGENTS.md/CLAUDE.md (`--no-context-files`
// disables the latter) run into several KB on every single turn of the session —
// wasteful here since the actual task is already fully spelled out in `prompt`
// (the first user message). A short replacement keeps `pi` framed as a coding
// agent without resending that boilerplate on each call.
const SYSTEM_PROMPT: &str = "Ти кодинг-асистент із read/bash/edit/write доступом до поточної директорії. Аналізуй запит користувача і, де це доречно, вноси зміни в код проєкту.";

#[derive(Debug, Error)]
pub enum PiAgentError {
    #[error("не вдалось запустити `pi`: {0}")]
    Spawn(#[from] tauri_plugin_shell::Error),
    #[error("pi завершився з кодом {code:?}: {stderr}")]
    NonZeroExit { code: Option<i32>, stderr: String },
}

impl From<PiAgentError> for String {
    fn from(value: PiAgentError) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PiModelOption {
    /// Ім'я env-змінної (напр. `N_CLOUD_MAX_MODEL`) — стабільний ключ для UI.
    pub key: String,
    /// Похідна читабельна мітка (напр. `CLOUD MAX`).
    pub label: String,
    /// Значення для `pi --model` (напр. `openai-codex/gpt-5.5`).
    pub model: String,
}

/// Знаходить усі `N_*_MODEL` env-змінні процесу — той самий підхід, що
/// `omlx_env_api_key` (`omlx/admin.rs`), але без хардкоду конкретних імен: нову
/// модель у списку користувач додає просто новою env-змінною за конвенцією
/// `N_<TIER>_MODEL`, без змін коду. Значення мають формат `pi`-CLI
/// `--model <provider/id>` (напр. `openai-codex/gpt-5.5`).
#[tauri::command]
pub fn pi_agent_models() -> Vec<PiModelOption> {
    pi_agent_models_from_env(std::env::vars())
}

fn pi_agent_models_from_env(
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<PiModelOption> {
    let mut options: Vec<PiModelOption> = vars
        .into_iter()
        .filter(|(key, model)| {
            key.starts_with("N_") && key.ends_with("_MODEL") && !model.is_empty()
        })
        .map(|(key, model)| {
            let label = key
                .trim_start_matches("N_")
                .trim_end_matches("_MODEL")
                .replace('_', " ");
            PiModelOption { key, label, model }
        })
        .collect();
    options.sort_by(|a, b| a.key.cmp(&b.key));
    options
}

#[tauri::command]
pub async fn run_pi_agent(
    app: tauri::AppHandle,
    cwd: String,
    model: String,
    session_id: String,
    prompt: String,
) -> Result<String, String> {
    let output = app
        .shell()
        .command("pi")
        .args([
            "--print",
            "--model",
            &model,
            "--session-id",
            &session_id,
            "--no-context-files",
            "--system-prompt",
            SYSTEM_PROMPT,
            &prompt,
        ])
        .current_dir(cwd)
        .output()
        .await
        .map_err(PiAgentError::Spawn)?;

    if !output.status.success() {
        return Err(PiAgentError::NonZeroExit {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn finds_all_n_prefixed_model_vars_sorted_by_key() {
        let options = pi_agent_models_from_env(vars(&[
            ("N_CLOUD_MAX_MODEL", "openai-codex/gpt-5.5"),
            ("N_LOCAL_MIN_MODEL", "omlx/gemma-4-e4b-it-OptiQ-4bit"),
            ("N_CLOUD_MIN_MODEL", "openai-codex/gpt-5.4-mini"),
            ("N_CLOUD_AVG_MODEL", "openai-codex/gpt-5.4"),
            ("OMLX_API_KEY", "sample-secret"),
            ("PATH", "/usr/bin"),
        ]));
        assert_eq!(
            options,
            vec![
                PiModelOption {
                    key: "N_CLOUD_AVG_MODEL".to_string(),
                    label: "CLOUD AVG".to_string(),
                    model: "openai-codex/gpt-5.4".to_string(),
                },
                PiModelOption {
                    key: "N_CLOUD_MAX_MODEL".to_string(),
                    label: "CLOUD MAX".to_string(),
                    model: "openai-codex/gpt-5.5".to_string(),
                },
                PiModelOption {
                    key: "N_CLOUD_MIN_MODEL".to_string(),
                    label: "CLOUD MIN".to_string(),
                    model: "openai-codex/gpt-5.4-mini".to_string(),
                },
                PiModelOption {
                    key: "N_LOCAL_MIN_MODEL".to_string(),
                    label: "LOCAL MIN".to_string(),
                    model: "omlx/gemma-4-e4b-it-OptiQ-4bit".to_string(),
                },
            ]
        );
    }

    #[test]
    fn ignores_empty_values_and_non_matching_keys() {
        let options = pi_agent_models_from_env(vars(&[
            ("N_CLOUD_MAX_MODEL", ""),
            ("N_WEIRD_OTHER", "value"),
            ("SOME_MODEL", "value"),
        ]));
        assert!(options.is_empty());
    }
}
