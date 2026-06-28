import { ref } from 'vue'

const apiBase = import.meta.env.VITE_API_URL || 'http://127.0.0.1:3000'

export class ApiError extends Error {
  status: number
  code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}

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
          const code = body.error || 'unknown'
          const message = body.message || code
          if (res.status >= 400 && res.status < 500) {
            throw new ApiError(res.status, code, message)
          }
          throw new ApiError(res.status, code, message)
        }
        return body
      } catch (e: any) {
        lastError = e
        if (attempt < retries && (e.message === 'Failed to fetch' || (e instanceof ApiError && e.status >= 500))) {
          const delay = Math.pow(2, attempt) * 200
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
