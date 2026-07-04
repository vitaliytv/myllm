/**
 * Формує перше повідомлення pi-сесії для аналізу одного запису історії
 * проксі-запитів: контекст (шлях, модель, статус, тривалість) + тіло запиту
 * та відповіді + задача адаптувати код проєкту в поточній директорії.
 * @param {{path?: string, model?: string, status?: number, durationMs?: number, requestBody?: unknown, responseText?: string}} entry запис `RequestLogEntry`
 * @returns {string} готовий текст першого повідомлення (редагований користувачем перед відправкою)
 */
export function buildAnalysisPrompt(entry) {
  const meta = [
    entry.path && `шлях: ${entry.path}`,
    entry.model && `модель: ${entry.model}`,
    entry.status != null && `статус: ${entry.status}`,
    entry.durationMs != null && `тривалість: ${entry.durationMs}ms`
  ]
    .filter(Boolean)
    .join(', ')

  return [
    `Проаналізуй цей HTTP-запит і відповідь, що пройшли через локальний omlx-проксі (${meta}).`,
    '',
    'Запит:',
    JSON.stringify(entry.requestBody ?? null, null, 2),
    '',
    'Відповідь:',
    entry.responseText ?? '',
    '',
    'Подивись на код проєкту в поточній директорії та запропонуй (і, де це доречно, ' +
      'застосуй) зміни, які зроблять обробку такого запиту ефективнішою. Коротко ' +
      'поясни, що саме змінив і чому.'
  ].join('\n')
}
