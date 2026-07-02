import { invoke } from '@tauri-apps/api/core'

/**
 * Керує локальним reverse-proxy тасом (Rust `proxy.rs`): старт/стоп, і порт,
 * на якому він реально піднявся (може відрізнятись від запитаного, якщо той
 * зайнятий і викликач попросив `port: 0`).
 * @returns {object} { running, port, error, start(), stop() }
 */
export function useProxy() {
  const running = ref(false)
  const port = ref(null)
  const lastError = ref('')

  /**
   * Піднімає проксі-таск на бекенді й запам'ятовує реальний порт.
   * @param {string} upstreamBaseUrl справжній omlx (напр. http://127.0.0.1:8000)
   * @param {number} requestedPort бажаний локальний порт (0 = system-assigned)
   * @returns {Promise<void>}
   */
  async function start(upstreamBaseUrl, requestedPort) {
    lastError.value = ''
    try {
      port.value = await invoke('proxy_start', { upstreamBaseUrl, port: requestedPort })
      running.value = true
    } catch (error) {
      running.value = false
      lastError.value = String(error)
    }
  }

  /** Зупиняє проксі-таск на бекенді. */
  async function stop() {
    try {
      await invoke('proxy_stop')
    } catch (error) {
      lastError.value = String(error)
    } finally {
      running.value = false
    }
  }

  return { running, port, error: lastError, start, stop }
}
