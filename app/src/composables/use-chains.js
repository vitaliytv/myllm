import { invoke } from '@tauri-apps/api/core'
import { parseChainRecord, parseChainStep } from '../services/chains.js'

/**
 * Ланцюжки LLM-викликів із глобального trace `@nitra/llm-lib`
 * (`~/.n-cursor/llm-trace.jsonl`, читає Rust-команда `chains_list`).
 * Кроки ланцюжка вантажаться ліниво при розгортанні і кешуються.
 * @returns {object} { chains, analyses, error, load(), loadSteps(chainId), loadAnalyses() }
 */
export function useChains() {
  const chains = ref([])
  const analyses = ref([])
  const error = ref('')
  const stepsCache = new Map()

  /** Перезавантажує список ланцюжків (новіші зверху). */
  async function load() {
    error.value = ''
    try {
      const raw = await invoke('chains_list', { limit: 200 })
      chains.value = raw.map(r => parseChainRecord(r)).toReversed()
      stepsCache.clear()
    } catch (error) {
      error.value = String(error?.message ?? error)
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

  return { chains, analyses, error, load, loadSteps, loadAnalyses }
}
