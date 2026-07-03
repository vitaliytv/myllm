import { onMounted, onUnmounted } from 'vue'
import { relaunch } from '@tauri-apps/plugin-process'
import { check } from '@tauri-apps/plugin-updater'
import { useQuasar } from 'quasar'

/** Затримка першої перевірки після старту — щоб не гальмувати запуск UI. */
const FIRST_CHECK_DELAY_MS = 3000
/** Період фонових перевірок: довгоживучий інстанс теж дізнається про реліз. */
const CHECK_INTERVAL_MS = 60 * 60 * 1000

/**
 * Перевіряє оновлення при старті й далі щогодини; після встановлення
 * пропонує одразу перезапуститись у нову версію (relaunch).
 */
export function useUpdater() {
  const $q = useQuasar()
  let timer = null
  // Діалог показано або встановлення вже виконано — фонові чеки не дублюємо.
  let busy = false

  /** Питає GitHub releases про нову версію і веде користувача через install-flow. */
  async function checkForUpdates() {
    if (busy) return
    try {
      const update = await check()
      if (!update) return

      busy = true
      $q.dialog({
        title: 'Доступне оновлення',
        message: `Версія ${update.version} готова. Встановити зараз?`,
        cancel: { label: 'Пізніше', flat: true },
        ok: { label: 'Встановити', color: 'primary' },
        persistent: true,
      })
        .onOk(() => installAndRelaunch(update))
        .onCancel(() => {
          busy = false
        })
    } catch (error) {
      // Нема мережі або updater не налаштовано — не турбуємо користувача,
      // але лишаємо слід для діагностики (раніше тут був мовчазний catch).
      console.error('[updater] check failed:', error)
    }
  }

  /**
   * Завантажує і встановлює оновлення з прогресом, потім пропонує перезапуск.
   * @param {import('@tauri-apps/plugin-updater').Update} update знайдене оновлення
   * @returns {Promise<void>}
   */
  async function installAndRelaunch(update) {
    let downloaded = 0
    let total = 0
    const dismiss = $q.notify({ group: false, timeout: 0, spinner: true, message: 'Завантаження оновлення…' })
    try {
      await update.downloadAndInstall(event => {
        switch (event.event) {
        case 'Started': {
          total = event.data.contentLength ?? 0

        break;
        }
        case 'Progress': {
          downloaded += event.data.chunkLength
          if (total) dismiss({ message: `Завантаження… ${Math.round((downloaded / total) * 100)}%` })

        break;
        }
        case 'Finished': {
          dismiss()

        break;
        }
        // No default
        }
      })
      // busy лишається true: оновлення вже на диску, повторний чек лише
      // запропонував би встановити те саме ще раз.
      $q.dialog({
        title: 'Оновлення встановлено',
        message: `Перезапустити зараз, щоб перейти на версію ${update.version}?`,
        cancel: { label: 'Пізніше', flat: true },
        ok: { label: 'Перезапустити', color: 'primary' },
        persistent: true,
      }).onOk(() => relaunch())
    } catch (error) {
      dismiss()
      busy = false
      console.error('[updater] install failed:', error)
      $q.notify({ message: `Помилка оновлення: ${error}`, color: 'negative', timeout: 5000 })
    }
  }

  onMounted(() => {
    setTimeout(checkForUpdates, FIRST_CHECK_DELAY_MS)
    timer = setInterval(checkForUpdates, CHECK_INTERVAL_MS)
  })

  onUnmounted(() => {
    if (timer) clearInterval(timer)
  })
}
