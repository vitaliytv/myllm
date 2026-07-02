import { invoke } from '@tauri-apps/api/core'
import { parseMaxConcurrent, parseQueueSnapshot } from '../services/omlx-queue.js'

const POLL_INTERVAL_MS = 1500

/**
 * Жива черга omlx: логінить admin-сесію один раз, далі опитує
 * `/admin/api/stats` кожні 1.5с. `connect()` викликає composable-споживач
 * явно (форма підключення), не автоматично при монтуванні — щоб не бити в
 * порожній baseUrl/apiKey до того, як їх ввели.
 * @returns {object} реактивний стан + connect()/disconnect()
 */
export function useOmlxQueue() {
  const connected = ref(false)
  const connecting = ref(false)
  const lastError = ref('')
  const snapshot = ref({ totalRequests: 0, models: [] })
  const maxConcurrent = ref(null)
  let intervalId = null

  /** Один запит стану черги + глобальних налаштувань, оновлює реактивний стан. */
  async function poll() {
    try {
      const [stats, globalSettings] = await Promise.all([
        invoke('omlx_stats'),
        invoke('omlx_global_settings')
      ])
      snapshot.value = parseQueueSnapshot(stats)
      maxConcurrent.value = parseMaxConcurrent(globalSettings)
      lastError.value = ''
    } catch (error) {
      lastError.value = String(error)
    }
  }

  /**
   * Логінить admin-сесію і запускає періодичний poll.
   * @param {string} baseUrl upstream omlx URL (напр. http://127.0.0.1:8000)
   * @param {string} apiKey admin API-ключ
   * @returns {Promise<void>}
   */
  async function connect(baseUrl, apiKey) {
    connecting.value = true
    lastError.value = ''
    try {
      await invoke('omlx_connect', { baseUrl, apiKey })
      connected.value = true
      await poll()
      intervalId = setInterval(poll, POLL_INTERVAL_MS)
    } catch (error) {
      connected.value = false
      lastError.value = String(error)
    } finally {
      connecting.value = false
    }
  }

  /** Зупиняє poll-інтервал (сесія на бекенді лишається — просто перестаємо питати). */
  function disconnect() {
    if (intervalId) clearInterval(intervalId)
    intervalId = null
    connected.value = false
  }

  onUnmounted(disconnect)

  return { connected, connecting, error: lastError, snapshot, maxConcurrent, connect, disconnect }
}
