import { ref } from 'vue'

const apiBase = import.meta.env.VITE_API_URL || 'http://127.0.0.1:3000'

export function useApi() {
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function request(path: string, options: RequestInit = {}) {
    loading.value = true
    error.value = null
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
      const body = await res.json()
      if (!res.ok) throw new Error(body.message || body.error || 'request failed')
      return body
    } catch (e: any) {
      error.value = e.message
      throw e
    } finally {
      loading.value = false
    }
  }

  function setToken(token: string) {
    localStorage.setItem('mathcoin_token', token)
  }

  function clearToken() {
    localStorage.removeItem('mathcoin_token')
  }

  return { loading, error, request, setToken, clearToken }
}
