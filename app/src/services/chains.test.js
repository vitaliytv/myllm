import { describe, expect, it } from 'vitest'
import {
  chainAggregates,
  chainProblemLabel,
  chainResolutionLabel,
  chainTouchedFilesLabel,
  isLocalModel,
  joinStepsWithBodies,
  parseChainRecord,
  parseChainStep
} from './chains.js'

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

  it('зберігає messages/content для перегляду промпту/відповіді кроку', () => {
    const step = parseChainStep({
      chainStep: 1,
      messages: [{ role: 'user', content: 'привіт' }],
      content: 'відповідь моделі'
    })
    expect(step.messages).toEqual([{ role: 'user', content: 'привіт' }])
    expect(step.content).toBe('відповідь моделі')
    expect(parseChainStep(null)).toMatchObject({ messages: [], content: null })
    expect(parseChainStep({ messages: 'не масив' }).messages).toEqual([])
  })
})

describe('isLocalModel', () => {
  it('провайдер omlx — локальна модель', () => {
    expect(isLocalModel('omlx/gemma-4-e4b-it')).toBe(true)
  })

  it('інший провайдер — cloud', () => {
    expect(isLocalModel('openai/gpt-5.4-mini')).toBe(false)
  })

  it('порожнє/невалідне значення — false, не падає', () => {
    expect(isLocalModel(null)).toBe(false)
    expect(isLocalModel()).toBe(false)
    expect(isLocalModel('')).toBe(false)
  })
})

describe('підписи шапки ланцюжка (extra-конвенція producer-а)', () => {
  it('chainProblemLabel: кількість · reasons · sample; без problem — порожньо', () => {
    const extra = {
      problem: { violations: 3, reasons: ['crc-mismatch', 'missing'], files: ['a.md'], sample: 'CRC не збігся' }
    }
    expect(chainProblemLabel(extra)).toBe('3 порушень · crc-mismatch, missing · CRC не збігся')
    expect(chainProblemLabel({ problem: { violations: 1 } })).toBe('1 порушень')
    expect(chainProblemLabel({})).toBe('')
    expect(chainProblemLabel(null)).toBe('')
  })

  it('chainResolutionLabel: t0 → T0, інакше tier:model, без resolvedBy — порожньо', () => {
    expect(chainResolutionLabel({ resolvedBy: 't0' })).toBe('T0')
    expect(chainResolutionLabel({ resolvedBy: 'cloud-min:openai/gpt-5.4-mini' })).toBe('cloud-min:openai/gpt-5.4-mini')
    expect(chainResolutionLabel({})).toBe('')
    expect(chainResolutionLabel()).toBe('')
  })

  it('chainTouchedFilesLabel: список + хвіст (+N) з touchedTotal', () => {
    expect(chainTouchedFilesLabel({ touchedFiles: ['a.js', 'b.js'], touchedTotal: 5 })).toBe('a.js, b.js (+3)')
    expect(chainTouchedFilesLabel({ touchedFiles: ['a.js'] })).toBe('a.js')
    expect(chainTouchedFilesLabel({ touchedFiles: [] })).toBe('')
    expect(chainTouchedFilesLabel(null)).toBe('')
  })
})

describe('joinStepsWithBodies', () => {
  const steps = [
    parseChainStep({ chainStep: 1, model: 'omlx/g', promptHash: 'h1' }),
    parseChainStep({ chainStep: 2, model: 'openai/gpt', promptHash: 'h2' })
  ]

  it('primary-джойн за chainStep, працює й для cloud-кроків', () => {
    const joined = joinStepsWithBodies(steps, [
      { chainStep: 1, prompt: 'p1', output: 'o1' },
      { chainStep: 2, prompt: 'p2', output: 'o2' }
    ])
    expect(joined[0].body).toEqual({ prompt: 'p1', output: 'o1' })
    expect(joined[1].body).toEqual({ prompt: 'p2', output: 'o2' })
  })

  it('fallback за promptHash, без матчу — body:null', () => {
    const joined = joinStepsWithBodies(steps, [{ promptHash: 'h1', prompt: 'p1', output: 'o1' }])
    expect(joined[0].body).toEqual({ prompt: 'p1', output: 'o1' })
    expect(joined[1].body).toBeNull()
  })

  it('порожній bodies — усі body:null, не падає', () => {
    expect(joinStepsWithBodies(steps, [])).toEqual([{ ...steps[0], body: null }, { ...steps[1], body: null }])
    expect(joinStepsWithBodies(steps)[0].body).toBeNull()
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
