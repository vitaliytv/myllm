import { describe, expect, it } from 'vitest'
import { DEFAULT_CONNECTION, loadConnection, saveConnection, STORAGE_KEY } from './omlx-connection.js'

/** @returns {Storage} мінімальний in-memory Storage-мок для тестів */
function fakeStorage() {
  const map = new Map()
  return {
    getItem: key => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, value)
  }
}

describe('loadConnection', () => {
  it('returns defaults when storage is missing or empty', () => {
    expect(loadConnection()).toEqual(DEFAULT_CONNECTION)
    expect(loadConnection(fakeStorage())).toEqual(DEFAULT_CONNECTION)
  })

  it('returns defaults when stored value is not valid JSON', () => {
    const storage = fakeStorage()
    storage.setItem(STORAGE_KEY, 'not json')
    expect(loadConnection(storage)).toEqual(DEFAULT_CONNECTION)
  })

  it('merges saved fields over defaults', () => {
    const storage = fakeStorage()
    storage.setItem(STORAGE_KEY, JSON.stringify({ apiKey: 'secret' }))
    expect(loadConnection(storage)).toEqual({ ...DEFAULT_CONNECTION, apiKey: 'secret' })
  })
})

describe('saveConnection', () => {
  it('round-trips through loadConnection', () => {
    const storage = fakeStorage()
    const connection = { baseUrl: 'http://127.0.0.1:8000', apiKey: 'k', proxyPort: 9000 }
    saveConnection(storage, connection)
    expect(loadConnection(storage)).toEqual(connection)
  })

  it('is a no-op without a usable storage', () => {
    expect(() => saveConnection(undefined, DEFAULT_CONNECTION)).not.toThrow()
  })
})
