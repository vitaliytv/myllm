// Побудова промпта для LLM-аналізу цілого ланцюжка (вкладка «Ланцюжки»):
// модель отримує таблицю кроків з приджойненими даними проксі і завдання —
// маючи read-only доступ до коду в cwd виклику, знайти причину неефективності
// та запропонувати зміни до інструмента-джерела (@nitra/cursor / @7n/test)
// у форматі самодостатнього патч-промпта (стиль n-llm-patch).

/** Мапа chainKind → репозиторій-джерело інструмента. */
const TARGET_REPOS = {
  'fix-concern': '@nitra/cursor',
  'doc-generate': '@nitra/cursor',
  'adr-normalize': '@nitra/cursor',
  'mutant-classify': '@7n/test',
  'test-generate': '@7n/test',
  'test-fix': '@7n/test'
}

/**
 * Визначає репозиторій-джерело за типом задачі ланцюжка.
 * @param {string} chainKind тип задачі
 * @returns {string} назва пакета-джерела або '(невідомий інструмент)'
 */
export function inferTargetRepo(chainKind) {
  return TARGET_REPOS[chainKind] ?? '(невідомий інструмент)'
}

/**
 * Рядок таблиці кроків для промпта.
 * @param {object} step крок після joinStepsWithRequests
 * @returns {string} markdown-рядок таблиці
 */
function stepRow(step) {
  const where = step.cloud ? 'cloud' : 'local'
  const tokens = step.usage?.totalTokens ?? '—'
  const duration = step.request ? `${step.request.durationMs}ms` : '—'
  const error = step.error ? String(step.error).slice(0, 120) : ''
  return `| ${step.chainStep} | ${step.kind} | ${step.model ?? '—'} | ${where} | ${tokens} | ${duration} | ${error} |`
}

/**
 * Будує промпт аналізу ланцюжка.
 * @param {{chain: object, steps: Array<object>}} args нормалізований ланцюжок + кроки (після join)
 * @returns {string} текст промпта для pi-сесії у cwd ланцюжка
 */
export function buildChainAnalysisPrompt({ chain, steps }) {
  const targetRepo = inferTargetRepo(chain.chainKind)
  const stepsTable = [
    '| # | kind | model | де | tokens | час | помилка |',
    '| --- | --- | --- | --- | --- | --- | --- |',
    ...(steps ?? []).map(stepRow)
  ].join('\n')

  return [
    `Проаналізуй ланцюжок LLM-викликів задачі \`${chain.chainKind}/${chain.unit}\` (chainId ${chain.chainId}).`,
    '',
    `Підсумок: outcome=${chain.outcome}, кроків ${chain.steps}, локальних викликів ${chain.localCalls}, хмарних ${chain.cloudCalls}${chain.escalated ? ' (БУЛА ескалація local→cloud)' : ''}, тривалість ${Math.round(chain.wallMs / 1000)}s, cloud-токенів ${chain.usageCloud?.totalTokens ?? 0}.`,
    '',
    '## Кроки',
    stepsTable,
    '',
    `Ти працюєш у директорії, де виконувалась задача (\`${chain.cwd ?? 'невідома'}\`) — код доступний read-only, НІЧОГО не змінюй.`,
    '',
    `## Завдання`,
    `1. Знайди причину неефективності: чому локальна модель не впоралась / чому знадобилась ескалація в cloud / чому кроків більше ніж треба. Подивись реальні файли з цієї директорії, що фігурують у задачі.`,
    `2. Запропонуй зміни до інструмента-джерела (${targetRepo}), які зменшать хмарні виклики або підвищать якість локальної обробки: кращий промпт правила, детермінований T0-фіксер (fix-*.mjs), точніший детектор, бюджет промпта.`,
    `3. Відповідь оформи як САМОДОСТАТНІЙ патч-промпт для агента, що працюватиме в репо ${targetRepo} (стиль n-llm-patch): контекст, конкретні файли/функції, приклади з цієї директорії як тест-фікстури, критерій готовності.`
  ].join('\n')
}
