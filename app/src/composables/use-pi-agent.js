import { invoke } from '@tauri-apps/api/core'

/**
 * Тонка обгортка над Rust-командами `pi_agent.rs`: список моделей будується з
 * усіх `N_*_MODEL` env-змінних процесу (нову модель додає сам користувач
 * новою env-змінною, без змін коду), а `send()` запускає один хід pi-сесії
 * (`pi --print --session-id <id> ...`) у cwd процесу-клієнта.
 * @returns {object} { models, loadModels(), send() }
 */
export function usePiAgent() {
  const models = ref([]) // [{key, label, model}]

  /** Підтягує список `{key, label, model}` з `N_*_MODEL` env-змінних бекенд-процесу. */
  async function loadModels() {
    models.value = await invoke('pi_agent_models')
  }

  /**
   * Один хід чату з `pi`: продовжує сесію `sessionId` тим самим `--session-id`.
   * @param {{cwd: string, model: string, sessionId: string, prompt: string}} params
   * @returns {Promise<string>} фінальний текст відповіді pi (stdout)
   */
  async function send({ cwd, model, sessionId, prompt }) {
    return invoke('run_pi_agent', { cwd, model, sessionId, prompt })
  }

  return { models, loadModels, send }
}
