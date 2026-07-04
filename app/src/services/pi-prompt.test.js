import { describe, expect, it } from 'vitest'
import { buildAnalysisPrompt } from './pi-prompt.js'

describe('buildAnalysisPrompt', () => {
  it('includes path/model/status/duration meta line', () => {
    const prompt = buildAnalysisPrompt({
      path: '/v1/chat/completions',
      model: 'gemma-4-e4b-it-OptiQ-4bit',
      status: 200,
      durationMs: 842,
      requestBody: { messages: [] },
      responseText: 'hi'
    })
    expect(prompt).toContain('шлях: /v1/chat/completions')
    expect(prompt).toContain('модель: gemma-4-e4b-it-OptiQ-4bit')
    expect(prompt).toContain('статус: 200')
    expect(prompt).toContain('тривалість: 842ms')
  })

  it('embeds pretty-printed request body and response text', () => {
    const prompt = buildAnalysisPrompt({
      requestBody: { model: 'x', messages: [{ role: 'user', content: 'hi' }] },
      responseText: 'відповідь моделі'
    })
    expect(prompt).toContain('"model": "x"')
    expect(prompt).toContain('відповідь моделі')
  })

  it('tolerates missing fields without throwing', () => {
    expect(() => buildAnalysisPrompt({})).not.toThrow()
    const prompt = buildAnalysisPrompt({})
    expect(prompt).toContain('null')
  })
})
