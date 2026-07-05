import { describe, expect, it } from 'vitest'
import { chainAggregates, groupEntriesByCorrelation, joinStepsWithRequests, parseChainRecord, parseChainStep } from './chains.js'

const chain = (over = {}) => parseChainRecord({
  ts: '2026-07-05T10:00:00.000Z',
  chainId: 'c1',
  chainKind: 'fix-concern',
  unit: 'text/cspell',
  outcome: 'success',
  steps: 2,
  localCalls: 1,
  cloudCalls: 1,
  escalated: true,
  usageCloud: { totalTokens: 100 },
  wallMs: 500,
  ...over
})

describe('parseChainRecord/parseChainStep', () => {
  it('повний запис нормалізується, мінімальний отримує дефолти', () => {
    expect(chain().chainKind).toBe('fix-concern')
    const minimal = parseChainRecord({})
    expect(minimal).toMatchObject({ chainId: '', outcome: 'fail', steps: 0, escalated: false, extra: {} })
    expect(parseChainStep({ chainStep: 3, model: 'omlx/x' })).toMatchObject({ chainStep: 3, model: 'omlx/x', error: null })
    expect(parseChainStep(null).chainStep).toBe(0)
  })
})

describe('groupEntriesByCorrelation', () => {
  it('групує за correlationId, одиночні без нього — групи розміру 1', () => {
    const entries = [
      { id: 3, correlationId: 'a', chainKind: 'fix-concern', durationMs: 10, model: 'm1' },
      { id: 2, durationMs: 5, model: 'm2' },
      { id: 1, correlationId: 'a', durationMs: 20, model: 'm1' }
    ]
    const groups = groupEntriesByCorrelation(entries)
    expect(groups).toHaveLength(2)
    expect(groups[0]).toMatchObject({ correlationId: 'a', totalDurationMs: 30, models: ['m1'] })
    expect(groups[0].entries).toHaveLength(2)
    expect(groups[1].correlationId).toBeNull()
  })

  it('порожній вхід → порожній список', () => {
    expect(groupEntriesByCorrelation([])).toEqual([])
    expect(groupEntriesByCorrelation(undefined)).toEqual([])
  })
})

describe('joinStepsWithRequests', () => {
  const steps = [
    parseChainStep({ chainStep: 1, model: 'omlx/g', promptHash: 'h1' }),
    parseChainStep({ chainStep: 2, model: 'openai/gpt', promptHash: 'h2' })
  ]

  it('primary-джойн за correlationId+chainStep', () => {
    const joined = joinStepsWithRequests(steps, 'c1', [
      { correlationId: 'c1', chainStep: 1, durationMs: 42, status: 200 }
    ])
    expect(joined[0].request).toMatchObject({ durationMs: 42, status: 200 })
    expect(joined[0].cloud).toBe(false)
    expect(joined[1].request).toBeNull()
    expect(joined[1].cloud).toBe(true)
  })

  it('fallback за promptHash, без матчу — cloud:true', () => {
    const joined = joinStepsWithRequests(steps, 'c1', [{ promptHash: 'h1', durationMs: 7 }])
    expect(joined[0].request).toMatchObject({ durationMs: 7 })
    expect(joined[1].cloud).toBe(true)
  })
})

describe('chainAggregates', () => {
  it('perKind з escalation-rate і totals', () => {
    const { perKind, totals } = chainAggregates([
      chain(),
      chain({ chainId: 'c2', outcome: 'fail', escalated: false, localCalls: 2, cloudCalls: 0, usageCloud: { totalTokens: 0 } }),
      chain({ chainId: 'c3', chainKind: 'doc-generate', unit: 'a.mjs', outcome: 'partial' })
    ])
    const fix = perKind.find(k => k.kind === 'fix-concern')
    expect(fix).toMatchObject({ chains: 2, success: 1, fail: 1, escalationRate: 0.5 })
    expect(totals).toMatchObject({ chains: 3, cloudCalls: 2, cloudTokens: 200 })
  })

  it('alwaysEscalatedUnits: поріг ≥3, 100% escalated/cloudOnly, сорт за cloudTokens', () => {
    const mk = (id, unit, over = {}) => chain({ chainId: id, unit, ...over })
    const { alwaysEscalatedUnits } = chainAggregates([
      mk('a1', 'ga/pins'), mk('a2', 'ga/pins'), mk('a3', 'ga/pins'),
      mk('b1', 'js/x'), mk('b2', 'js/x'), mk('b3', 'js/x', { escalated: false, cloudCalls: 0, localCalls: 1, usageCloud: { totalTokens: 0 } }),
      mk('d1', 'npm/pub', { escalated: false, localCalls: 0, usageCloud: { totalTokens: 900 } }),
      mk('d2', 'npm/pub', { escalated: false, localCalls: 0, usageCloud: { totalTokens: 900 } }),
      mk('d3', 'npm/pub', { escalated: false, localCalls: 0, usageCloud: { totalTokens: 900 } })
    ])
    expect(alwaysEscalatedUnits.map(u => u.unit)).toEqual(['npm/pub', 'ga/pins'])
  })

  it('фільтр періоду sinceMs і порожній вхід', () => {
    const old = chain({ chainId: 'old', ts: '2026-07-01T00:00:00.000Z' })
    const fresh = chain({ chainId: 'new' })
    const { totals } = chainAggregates([old, fresh], { sinceMs: Date.parse('2026-07-04T00:00:00.000Z') })
    expect(totals.chains).toBe(1)
    expect(chainAggregates([]).totals.chains).toBe(0)
  })
})
