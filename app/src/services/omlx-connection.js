// Зберігає налаштування підключення (upstream omlx URL, admin API-ключ,
// локальний порт проксі) між сесіями — один JSON-запис у localStorage.

export const STORAGE_KEY = 'myllm.connection'

export const DEFAULT_CONNECTION = {
  baseUrl: 'http://127.0.0.1:8000',
  apiKey: '',
  proxyPort: 8088
}

/**
 * Читає збережене підключення. Повертає дефолти, якщо storage порожній,
 * недоступний або містить невалідний JSON.
 * @param {Storage|undefined} storage джерело (зазвичай window.localStorage)
 * @returns {{ baseUrl: string, apiKey: string, proxyPort: number }} підключення
 */
export function loadConnection(storage) {
  if (!storage || typeof storage.getItem !== 'function') return { ...DEFAULT_CONNECTION }
  const raw = storage.getItem(STORAGE_KEY)
  if (!raw) return { ...DEFAULT_CONNECTION }
  try {
    const parsed = JSON.parse(raw)
    return { ...DEFAULT_CONNECTION, ...parsed }
  } catch {
    return { ...DEFAULT_CONNECTION }
  }
}

/**
 * Записує підключення в storage.
 * @param {Storage|undefined} storage джерело (зазвичай window.localStorage)
 * @param {{ baseUrl: string, apiKey: string, proxyPort: number }} connection значення для запису
 */
export function saveConnection(storage, connection) {
  if (!storage || typeof storage.setItem !== 'function') return
  storage.setItem(STORAGE_KEY, JSON.stringify(connection))
}
