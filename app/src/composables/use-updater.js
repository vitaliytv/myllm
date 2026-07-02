import { onMounted } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { useQuasar } from 'quasar'

/** Перевіряє й, з підтвердження користувача, встановлює оновлення при старті. */
export function useUpdater() {
  const $q = useQuasar()

  /** Питає GitHub releases про нову версію і веде користувача через install-flow. */
  async function checkForUpdates() {
    try {
      const update = await check()
      if (!update) return

      $q.dialog({
        title: 'Доступне оновлення',
        message: `Версія ${update.version} готова. Встановити зараз?`,
        cancel: { label: 'Пізніше', flat: true },
        ok: { label: 'Встановити', color: 'primary' },
        persistent: true,
      }).onOk(async () => {
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
          $q.notify({ message: 'Оновлення встановлено. Перезапусти програму.', color: 'positive', timeout: 0, actions: [{ label: 'OK', color: 'white' }] })
        } catch (error) {
          dismiss()
          $q.notify({ message: `Помилка оновлення: ${error}`, color: 'negative', timeout: 5000 })
        }
      })
    } catch {
      // Нема мережі, Android, або updater не налаштовано — мовчки ігноруємо
    }
  }

  onMounted(() => {
    setTimeout(checkForUpdates, 3000)
  })
}
