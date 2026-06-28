import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

describe('useApi', () => {
  let store: Record<string, string>
  let fetchSpy: ReturnType<typeof vi.fn>
  let useApi: any

  beforeEach(async () => {
    store = {}
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => store[k] ?? null,
      setItem: (k: string, v: string) => { store[k] = v },
      removeItem: (k: string) => { delete store[k] },
    })
    fetchSpy = vi.fn()
    vi.stubGlobal('fetch', fetchSpy)
    vi.resetModules()
    useApi = (await import('../useApi')).useApi
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('setToken saves token to localStorage', () => {
    const api = useApi()
    api.setToken('jwt-token-123')
    expect(store['mathcoin_token']).toBe('jwt-token-123')
  })

  it('clearToken removes token from localStorage', () => {
    const api = useApi()
    api.setToken('jwt-token-123')
    api.clearToken()
    expect(store['mathcoin_token']).toBeUndefined()
  })

  it('request sends Authorization header when token is set', async () => {
    fetchSpy.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ email: 'deb@example.com', balance: 0 }),
    } as any)

    const api = useApi()
    api.setToken('jwt-token-123')
    await api.request('/api/me')

    expect(fetchSpy).toHaveBeenCalledTimes(1)
    const [url, opts] = fetchSpy.mock.calls[0]
    expect(url).toContain('/api/me')
    expect(opts.headers['Authorization']).toBe('Bearer jwt-token-123')
    expect(opts.headers['Content-Type']).toBe('application/json')
  })

  it('request throws on non-ok response', async () => {
    fetchSpy.mockResolvedValue({
      ok: false,
      status: 401,
      json: () => Promise.resolve({ error: 'unauthenticated', message: 'bad token' }),
    } as any)

    const api = useApi()
    await expect(api.request('/api/me')).rejects.toThrow('bad token')
    expect(api.error.value).toBe('bad token')
  })

  it('request does NOT send Authorization when no token', async () => {
    fetchSpy.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({}),
    } as any)

    const api = useApi()
    await api.request('/api/session', { method: 'POST' })

    const [_u, opts] = fetchSpy.mock.calls[0]
    expect(opts.headers['Authorization']).toBeUndefined()
  })
})
