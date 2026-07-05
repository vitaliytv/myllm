import { describe, expect, it } from 'vitest'
import { buildChainAnalysisPrompt, inferTargetRepo } from './chain-analysis.js'

describe('inferTargetRepo', () => {
  it('мапить kind на репо-джерело', () => {
    expect(inferTargetRepo('fix-concern')).toBe('@nitra/cursor')
    expect(inferTargetRepo('doc-generate')).toBe('@nitra/cursor')
    expect(inferTargetRepo('mutant-classify')).toBe('@7n/test')
    expect(inferTargetRepo('test-fix')).toBe('@7n/test')
    expect(inferTargetRepo('щось-нове')).toBe('(невідомий інструмент)')
  })
})

describe('buildChainAnalysisPrompt', () => {
  const chain = {
    chainId: 'abc123',
    chainKind: 'fix-concern',
    unit: 'text/cspell',
    cwd: '/Users/x/proj',
    outcome: 'success',
    steps: 2,
    localCalls: 1,
    cloudCalls: 1,
    escalated: true,
    wallMs: 12_000,
    usageCloud: { totalTokens: 345 }
  }
  const steps = [
    { chainStep: 1, kind: 'agent', model: 'omlx/gemma', cloud: false, usage: { totalTokens: 100 }, request: { durationMs: 900 }, error: 'досі порушено' },
    { chainStep: 2, kind: 'agent', model: 'openai/gpt-5.4-mini', cloud: true, usage: { totalTokens: 345 }, request: null, error: null }
  ]

  it('містить unit, cwd, targetRepo, таблицю кроків і маркер ескалації', () => {
    const p = buildChainAnalysisPrompt({ chain, steps })
    expect(p).toContain('fix-concern/text/cspell')
    expect(p).toContain('/Users/x/proj')
    expect(p).toContain('@nitra/cursor')
    expect(p).toContain('| 1 | agent | omlx/gemma | local | 100 | 900ms | досі порушено |')
    expect(p).toContain('| 2 | agent | openai/gpt-5.4-mini | cloud | 345 | — |  |')
    expect(p).toContain('БУЛА ескалація')
    expect(p).toContain('патч-промпт')
  })

  it('без кроків і cwd не падає', () => {
    const p = buildChainAnalysisPrompt({ chain: { ...chain, cwd: null, escalated: false }, steps: [] })
    expect(p).toContain('невідома')
    expect(p).not.toContain('БУЛА ескалація')
  })
})
