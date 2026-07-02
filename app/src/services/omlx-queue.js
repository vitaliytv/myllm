// Чисті функції парсингу сирого omlx `/admin/api/stats` та
// `/admin/api/global-settings` у view-модель для дашборда. Тримаються поза
// composable, щоб бути тестованими без Tauri/mock-invoke.

/**
 * Розбирає сиру відповідь /admin/api/stats у view-модель черги.
 * @param {unknown} rawStats відповідь GET /admin/api/stats
 * @returns {{ totalRequests: number, models: Array<object> }} черга по моделях
 */
export function parseQueueSnapshot(rawStats) {
  const models = rawStats?.active_models?.models ?? []
  return {
    totalRequests: rawStats?.total_requests ?? 0,
    models: models.map(model => parseModelEntry(model))
  }
}

/**
 * Розбирає один запис `active_models.models[]` у view-модель моделі.
 * @param {unknown} model один елемент active_models.models
 * @returns {object} нормалізована модель для UI
 */
function parseModelEntry(model) {
  return {
    id: model?.id ?? 'unknown',
    active: model?.active_requests ?? 0,
    waiting: model?.waiting_requests ?? 0,
    waitingList: (model?.waiting ?? []).map(w => ({
      requestId: w?.request_id ?? '',
      queuePosition: w?.queue_position ?? 0,
      elapsedSeconds: w?.elapsed_seconds ?? 0,
      promptTokens: w?.prompt_tokens ?? 0
    })),
    generatingList: (model?.generating ?? []).map(g => ({
      requestId: g?.request_id ?? '',
      elapsedSeconds: g?.elapsed_seconds ?? 0,
      generatedTokens: g?.generated_tokens ?? 0,
      tokensPerSecond: g?.tokens_per_second ?? 0,
      promptTokens: g?.prompt_tokens ?? 0,
      maxTokens: g?.max_tokens ?? 0
    }))
  }
}

/**
 * Витягує scheduler.max_concurrent_requests із global-settings.
 * @param {unknown} rawGlobalSettings відповідь GET /admin/api/global-settings
 * @returns {number|null} ліміт одночасних запитів або null, якщо не заданий
 */
export function parseMaxConcurrent(rawGlobalSettings) {
  const value = rawGlobalSettings?.scheduler?.max_concurrent_requests
  return typeof value === 'number' ? value : null
}
