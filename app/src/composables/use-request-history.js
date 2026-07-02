import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const HISTORY_LIMIT = 200
const MAX_ENTRIES_IN_MEMORY = 500

/**
 * Історія завершених /v1/* запитів, які пройшли через локальний проксі:
 * початкове заповнення з `requests.jsonl` (`proxy_history`), далі — живий
 * приріст через подію `omlx-request-logged` (push, без polling).
 * @returns {object} { entries, load() } — entries: новіші зверху
 */
export function useRequestHistory() {
  const entries = ref([])
  let unlisten = null

  /** Перезаповнює список із `requests.jsonl` (найновіші зверху). */
  async function load() {
    const history = await invoke('proxy_history', { limit: HISTORY_LIMIT })
    entries.value = history.toReversed()
  }

  onMounted(async () => {
    await load()
    unlisten = await listen('omlx-request-logged', event => {
      entries.value = [event.payload, ...entries.value].slice(0, MAX_ENTRIES_IN_MEMORY)
    })
  })

  onUnmounted(() => {
    if (unlisten) unlisten()
  })

  return { entries, load }
}
