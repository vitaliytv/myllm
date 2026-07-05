// Чисті функції для вкладки «Ланцюжки»: парсинг chain-записів глобального
// trace @nitra/llm-lib, групування request-логу за correlationId, джойн кроків
// ланцюжка з локальними запитами проксі та агрегати для аналітики
// (escalation-rate, T0-кандидати). Поза composable — тестуються без Tauri.

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
    error: raw?.error ?? null
  }
}

/**
 * Групує записи request-логу за `correlationId`; записи без нього — групи
 * розміру 1. Порядок збережено (група якориться на найновішому записі).
 * @param {Array<object>} entries записи історії (новіші зверху)
 * @returns {Array<{correlationId: string|null, chainKind: string|null, entries: Array<object>, totalDurationMs: number, models: string[]}>} групи
 */
export function groupEntriesByCorrelation(entries) {
  const groups = []
  const byId = new Map()
  for (const entry of entries ?? []) {
    const id = entry?.correlationId ?? null
    if (!id) {
      groups.push({ correlationId: null, chainKind: null, entries: [entry], totalDurationMs: entry?.durationMs ?? 0, models: entry?.model ? [entry.model] : [] })
      continue
    }
    let group = byId.get(id)
    if (!group) {
      group = { correlationId: id, chainKind: entry?.chainKind ?? null, entries: [], totalDurationMs: 0, models: [] }
      byId.set(id, group)
      groups.push(group)
    }
    group.entries.push(entry)
    group.totalDurationMs += entry?.durationMs ?? 0
    if (entry?.model && !group.models.includes(entry.model)) group.models.push(entry.model)
  }
  return groups
}

/**
 * Джойнить кроки ланцюжка з локальними записами проксі: primary —
 * correlationId+chainStep, fallback — promptHash. Cloud-кроки myllm не
 * бачить — позначаються `cloud:true` без request-даних.
 * @param {Array<object>} steps нормалізовані кроки (parseChainStep)
 * @param {string} chainId id ланцюжка
 * @param {Array<object>} requestEntries записи історії проксі
 * @returns {Array<object>} кроки з приєднаним `request` (або null)
 */
export function joinStepsWithRequests(steps, chainId, requestEntries) {
  const entries = requestEntries ?? []
  return (steps ?? []).map(step => {
    const byId = entries.find(e => e?.correlationId === chainId && e?.chainStep === step.chainStep)
    const byHash = byId ?? (step.promptHash ? entries.find(e => e?.promptHash === step.promptHash) : null)
    const request = byHash
      ? { durationMs: byHash.durationMs ?? 0, status: byHash.status ?? null, promptCompressed: byHash.promptCompressed ?? false }
      : null
    return { ...step, request, cloud: !request }
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
  const filtered = (chains ?? []).filter(inRange)

  const perKindMap = new Map()
  const perUnitMap = new Map()
  const totals = { chains: filtered.length, cloudCalls: 0, cloudTokens: 0 }
  for (const c of filtered) {
    let k = perKindMap.get(c.chainKind)
    if (!k) {
      k = { kind: c.chainKind, chains: 0, success: 0, partial: 0, fail: 0, escalated: 0, cloudCalls: 0, localCalls: 0, cloudTokens: 0, wallMs: 0 }
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

  const perKind = [...perKindMap.values()].map(k => ({
    ...k,
    escalationRate: k.chains ? k.escalated / k.chains : 0,
    avgWallMs: k.chains ? Math.round(k.wallMs / k.chains) : 0
  }))

  const alwaysEscalatedUnits = [...perUnitMap.values()]
    .filter(u => u.chains >= 3 && u.cloudCalls > 0 && u.escalated + u.cloudOnly === u.chains)
    .toSorted((a, b) => b.cloudTokens - a.cloudTokens || b.cloudCalls - a.cloudCalls)

  return { perKind, alwaysEscalatedUnits, totals }
}
