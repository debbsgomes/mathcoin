import { ref } from 'vue'

const apiBase = import.meta.env.VITE_API_URL || 'http://127.0.0.1:3000'

export function useApi() {
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function request(path: string, options: RequestInit = {}, retries = 2): Promise<any> {
    loading.value = true
    error.value = null
    let lastError: Error | null = null

    for (let attempt = 0; attempt <= retries; attempt++) {
      try {
        const token = localStorage.getItem('mathcoin_token')
        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
          ...(options.headers as Record<string, string>),
        }
        if (token) {
          headers['Authorization'] = `Bearer ${token}`
        }
        const res = await fetch(`${apiBase}${path}`, { ...options, headers })

        const contentType = res.headers.get('content-type') || ''
        if (!contentType.includes('application/json')) {
          throw new Error(`unexpected response type: ${contentType || 'none'}`)
        }

        const body = await res.json()
        if (!res.ok) {
          // Don't retry client errors (4xx) or auth failures
          if (res.status >= 400 && res.status < 500) {
            throw new Error(body.message || body.error || 'request failed')
          }
          throw new Error(body.message || body.error || 'server error')
        }
        return body
      } catch (e: any) {
        lastError = e
        // Only retry on network errors or 5xx
        if (attempt < retries && (e.message === 'Failed to fetch' || String(e.message).includes('server error'))) {
          const delay = Math.pow(2, attempt) * 200 // 200ms, 400ms
          await new Promise(r => setTimeout(r, delay))
          continue
        }
        break
      }
    }

    error.value = lastError?.message ?? 'request failed'
    loading.value = false
    throw lastError
  }

  function setToken(token: string) {
    localStorage.setItem('mathcoin_token', token)
  }

  function clearToken() {
    localStorage.removeItem('mathcoin_token')
  }

  return { loading, error, request, setToken, clearToken }
}
