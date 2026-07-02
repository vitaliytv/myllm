import { describe, expect, it } from 'vitest'
import { parseMaxConcurrent, parseQueueSnapshot } from './omlx-queue.js'

describe('parseQueueSnapshot', () => {
  it('returns empty models list when stats has no active_models', () => {
    expect(parseQueueSnapshot({})).toEqual({ totalRequests: 0, models: [] })
  })

  it('returns empty models list for null/undefined input', () => {
    expect(parseQueueSnapshot(null)).toEqual({ totalRequests: 0, models: [] })
    expect(parseQueueSnapshot()).toEqual({ totalRequests: 0, models: [] })
  })

  it('parses a full model entry with waiting and generating requests', () => {
    const raw = {
      total_requests: 60,
      active_models: {
        models: [
          {
            id: 'gemma-4-e4b-it-OptiQ-4bit',
            active_requests: 1,
            waiting_requests: 1,
            waiting: [
              { request_id: 'w1', queue_position: 1, elapsed_seconds: 0.86, prompt_tokens: 23968 }
            ],
            generating: [
              {
                request_id: 'g1',
                elapsed_seconds: 312.29,
                generated_tokens: 6039,
                tokens_per_second: 19.34,
                prompt_tokens: 1274,
                max_tokens: 32768
              }
            ]
          }
        ]
      }
    }

    const snapshot = parseQueueSnapshot(raw)
    expect(snapshot.totalRequests).toBe(60)
    expect(snapshot.models).toHaveLength(1)
    const [model] = snapshot.models
    expect(model.id).toBe('gemma-4-e4b-it-OptiQ-4bit')
    expect(model.active).toBe(1)
    expect(model.waiting).toBe(1)
    expect(model.waitingList).toEqual([
      { requestId: 'w1', queuePosition: 1, elapsedSeconds: 0.86, promptTokens: 23968 }
    ])
    expect(model.generatingList).toEqual([
      {
        requestId: 'g1',
        elapsedSeconds: 312.29,
        generatedTokens: 6039,
        tokensPerSecond: 19.34,
        promptTokens: 1274,
        maxTokens: 32768
      }
    ])
  })

  it('defaults missing per-model fields to zero/empty', () => {
    const raw = { active_models: { models: [{}] } }
    const [model] = parseQueueSnapshot(raw).models
    expect(model).toEqual({
      id: 'unknown',
      active: 0,
      waiting: 0,
      waitingList: [],
      generatingList: []
    })
  })
})

describe('parseMaxConcurrent', () => {
  it('reads scheduler.max_concurrent_requests', () => {
    expect(parseMaxConcurrent({ scheduler: { max_concurrent_requests: 8 } })).toBe(8)
  })

  it('returns null when missing or not a number', () => {
    expect(parseMaxConcurrent({})).toBeNull()
    expect(parseMaxConcurrent(null)).toBeNull()
    expect(parseMaxConcurrent({ scheduler: { max_concurrent_requests: '8' } })).toBeNull()
  })
})
