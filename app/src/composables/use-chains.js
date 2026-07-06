import { invoke } from '@tauri-apps/api/core'
import { parseChainRecord, parseChainStep } from '../services/chains.js'

/**
 * Повні тіла (prompt/response) opt-in body-capture стору llm-lib для
 * ланцюжка — порожній список, якщо `N_LLM_TRACE_BODIES` не було увімкнено
 * (не помилка, best-effort read).
 * @param {string} chainId id ланцюжка
 * @returns {Promise<Array<object>>} записи body-capture стору
 */
async function loadBodies(chainId) {
  try {
    return await invoke('read_body_capture', { chainId })
  } catch {
    return []
  }
}

/**
 * Ланцюжки LLM-викликів із глобального trace `@7n/llm-lib`
 * (`~/.n-cursor/llm-trace.jsonl`, читає Rust-команда `chains_list`).
 * Кроки ланцюжка вантажаться ліниво при розгортанні і кешуються.
 * @returns {object} { chains, analyses, error, load(), loadSteps(chainId), loadAnalyses() }
 */
export function useChains() {
  const chains = ref([])
  const analyses = ref([])
  // Не називаємо реф `error`, щоб `catch (error)` (canon unicorn/catch-error-name)
  // не затіняв його і повідомлення реально доходило до UI.
  const errorMessage = ref('')
  const stepsCache = new Map()

  /** Перезавантажує список ланцюжків (новіші зверху). */
  async function load() {
    errorMessage.value = ''
    try {
      const raw = await invoke('chains_list', { limit: 200 })
      chains.value = raw.map(r => parseChainRecord(r)).toReversed()
      stepsCache.clear()
    } catch (error) {
      errorMessage.value = String(error?.message ?? error)
    }
  }

  /**
   * Кроки одного ланцюжка (ліниво, з кешем на час життя списку).
   * @param {string} chainId id ланцюжка
   * @returns {Promise<Array<object>>} нормалізовані кроки
   */
  async function loadSteps(chainId) {
    if (stepsCache.has(chainId)) return stepsCache.get(chainId)
    const raw = await invoke('chain_steps', { chainId })
    const steps = raw.map(r => parseChainStep(r))
    stepsCache.set(chainId, steps)
    return steps
  }

  /** Індекс збережених аналізів (бейджі «має аналіз»). */
  async function loadAnalyses() {
    try {
      analyses.value = await invoke('list_chain_analyses')
    } catch {
      analyses.value = []
    }
  }

  onMounted(async () => {
    await load()
    await loadAnalyses()
  })

  return { chains, analyses, error: errorMessage, load, loadSteps, loadAnalyses, loadBodies }
}
