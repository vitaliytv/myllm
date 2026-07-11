// Чисті функції для вкладки «Ланцюжки»: парсинг chain-записів глобального
// trace @7n/llm-lib, джойн кроків ланцюжка з opt-in body-capture та агрегати
// для аналітики (escalation-rate, T0-кандидати). Поза composable — тестуються
// без Tauri.

/** Провайдери, що вважаються локальними — дзеркало llm-lib model-tiers.mjs (дефолт `omlx`). */
const LOCAL_PROVIDERS = new Set(['omlx'])

/**
 * Чи model-spec вказує на локальну модель (за префіксом провайдера
 * `provider/model-id`) — легкий UI-еквівалент `@7n/llm-lib/model-tiers`'
 * `isLocalModel`, без залежності від env-контракту (лише для відображення).
 * @param {string|null|undefined} spec `"provider/model-id"`
 * @returns {boolean} true — локальна модель
 */
export function isLocalModel(spec) {
  if (typeof spec !== 'string' || !spec) return false
  const provider = spec.split('/', 1)[0]
  return LOCAL_PROVIDERS.has(provider)
}

/**
 * Нормалізує фінальний запис ланцюжка (`kind:'chain'`) у view-модель.
 * @param {unknown} raw сирий запис trace
 * @returns {object} нормалізований ланцюжок
 */
export function parseChainRecord(raw) {
  return {
    ts: raw?.ts ?? null,
    chainId: raw?.chainId ?? '',
    chainKind: raw?.chainKind ?? '(без kind)',
    unit: raw?.unit ?? '',
    cwd: raw?.cwd ?? null,
    outcome: raw?.outcome ?? 'fail',
    steps: raw?.steps ?? 0,
    localCalls: raw?.localCalls ?? 0,
    cloudCalls: raw?.cloudCalls ?? 0,
    escalated: raw?.escalated ?? false,
    finalModel: raw?.finalModel ?? null,
    errors: raw?.errors ?? 0,
    wallMs: raw?.wallMs ?? 0,
    usage: raw?.usage ?? null,
    usageCloud: raw?.usageCloud ?? null,
    extra: raw?.extra ?? {}
  }
}

/**
 * Нормалізує per-call запис кроку ланцюжка.
 * @param {unknown} raw сирий запис trace
 * @returns {object} нормалізований крок
 */
export function parseChainStep(raw) {
  return {
    ts: raw?.ts ?? null,
    caller: raw?.caller ?? '',
    kind: raw?.kind ?? '',
    model: raw?.model ?? null,
    chainStep: raw?.chainStep ?? 0,
    usage: raw?.usage ?? null,
    stopReason: raw?.stopReason ?? null,
    promptHash: raw?.promptHash ?? null,
    error: raw?.error ?? null,
    // Trace-запис несе повний (можливо стиснутий клієнтом) промпт/відповідь —
    // primary джерело для перегляду «що питали / що відповіла модель» у UI,
    // доки body-capture (opt-in, нестиснуте) недоступне для цього кроку.
    messages: Array.isArray(raw?.messages) ? raw.messages : [],
    content: raw?.content ?? null
  }
}

/**
 * Підпис проблеми ланцюжка з `extra.problem` (конвенція producer-а, див.
 * llm-lib chain.mjs): скільки порушень, які reasons, приклад повідомлення.
 * @param {object|null|undefined} extra extra фінального chain-запису
 * @returns {string} людиночитний підпис або '' якщо проблему не зафіксовано
 */
export function chainProblemLabel(extra) {
  const p = extra?.problem
  if (!p) return ''
  const parts = [`${p.violations} порушень`]
  if (Array.isArray(p.reasons) && p.reasons.length > 0) parts.push(p.reasons.join(', '))
  if (p.sample) parts.push(p.sample)
  return parts.join(' · ')
}

/**
 * Підпис «хто закрив» з `extra.resolvedBy`: 'T0' для детермінованого патерну
 * (без LLM), інакше `tier:model` closing rung-а.
 * @param {object|null|undefined} extra extra фінального chain-запису
 * @returns {string} 'T0' | 'tier:model' | '' якщо ланцюжок не закрито
 */
export function chainResolutionLabel(extra) {
  const by = extra?.resolvedBy
  if (!by) return ''
  return by === 't0' ? 'T0' : by
}

/**
 * Підпис змінених файлів з `extra.touchedFiles` (cwd-relative, producer обрізає
 * список; `touchedTotal` несе повну кількість — хвіст показуємо як `+N`).
 * @param {object|null|undefined} extra extra фінального chain-запису
 * @returns {string} 'a.js, b.js (+3)' або '' якщо змін не зафіксовано
 */
export function chainTouchedFilesLabel(extra) {
  const files = Array.isArray(extra?.touchedFiles) ? extra.touchedFiles : []
  if (files.length === 0) return ''
  const more = (extra.touchedTotal ?? files.length) - files.length
  return files.join(', ') + (more > 0 ? ` (+${more})` : '')
}

/**
 * Приєднує повні тіла (prompt/response) opt-in body-capture стору llm-lib
 * (`read_body_capture`) до кроків — primary `chainStep`, fallback
 * `promptHash`. Незалежний від локального проксі (він тепер живе окремо,
 * `myllm-proxy-service`) — body-capture пише і для local, і для cloud.
 * @param {Array<object>} steps нормалізовані кроки (parseChainStep)
 * @param {Array<object>} bodies записи body-capture стору (read_body_capture)
 * @returns {Array<object>} кроки з приєднаним `body` (`{prompt, output}` або null)
 */
export function joinStepsWithBodies(steps, bodies) {
  const entries = bodies ?? []
  return (steps ?? []).map(step => {
    const byStep = entries.find(b => b?.chainStep === step.chainStep)
    const byHash = byStep ?? (step.promptHash ? entries.find(b => b?.promptHash === step.promptHash) : null)
    return { ...step, body: byHash ? { prompt: byHash.prompt ?? null, output: byHash.output ?? null } : null }
  })
}

/**
 * Агрегати для аналітики: perKind-метрики + юніти, що завжди ескалюють
 * (кандидати на T0-скрипти), з фільтром періоду.
 * @param {Array<object>} chains нормалізовані ланцюжки (parseChainRecord)
 * @param {{sinceMs?: number}} [opts] нижня межа за ts (epoch ms)
 * @returns {{perKind: Array<object>, alwaysEscalatedUnits: Array<object>, totals: object}} агрегати
 */
export function chainAggregates(chains, { sinceMs } = {}) {
  const inRange = c => !sinceMs || (c.ts && Date.parse(c.ts) >= sinceMs)
  const filtered = (chains ?? []).filter(c => inRange(c))

  const perKindMap = new Map()
  const perUnitMap = new Map()
  const totals = { chains: filtered.length, cloudCalls: 0, cloudTokens: 0 }
  for (const c of filtered) {
    let k = perKindMap.get(c.chainKind)
    if (!k) {
      k = {
        kind: c.chainKind,
        chains: 0,
        success: 0,
        partial: 0,
        fail: 0,
        escalated: 0,
        cloudCalls: 0,
        localCalls: 0,
        cloudTokens: 0,
        wallMs: 0
      }
      perKindMap.set(c.chainKind, k)
    }
    k.chains++
    if (c.outcome === 'success') k.success++
    else if (c.outcome === 'partial') k.partial++
    else k.fail++
    if (c.escalated) k.escalated++
    k.cloudCalls += c.cloudCalls
    k.localCalls += c.localCalls
    k.cloudTokens += c.usageCloud?.totalTokens ?? 0
    k.wallMs += c.wallMs
    totals.cloudCalls += c.cloudCalls
    totals.cloudTokens += c.usageCloud?.totalTokens ?? 0

    const unitKey = `${c.chainKind} ${c.unit}`
    let u = perUnitMap.get(unitKey)
    if (!u) {
      u = { kind: c.chainKind, unit: c.unit, chains: 0, escalated: 0, cloudOnly: 0, cloudCalls: 0, cloudTokens: 0 }
      perUnitMap.set(unitKey, u)
    }
    u.chains++
    if (c.escalated) u.escalated++
    if (c.cloudCalls > 0 && c.localCalls === 0) u.cloudOnly++
    u.cloudCalls += c.cloudCalls
    u.cloudTokens += c.usageCloud?.totalTokens ?? 0
  }

  const perKind = Array.from(perKindMap.values(), k => ({
    ...k,
    escalationRate: k.chains ? k.escalated / k.chains : 0,
    avgWallMs: k.chains ? Math.round(k.wallMs / k.chains) : 0
  }))

  const alwaysEscalatedUnits = perUnitMap
    .values()
    .filter(u => u.chains >= 3 && u.cloudCalls > 0 && u.escalated + u.cloudOnly === u.chains)
    .toArray()
    .toSorted((a, b) => b.cloudTokens - a.cloudTokens || b.cloudCalls - a.cloudCalls)

  return { perKind, alwaysEscalatedUnits, totals }
}
