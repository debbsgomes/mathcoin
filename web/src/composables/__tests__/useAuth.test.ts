import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'

const mockSignInWithPassword = vi.fn()
const mockSignInWithOAuth = vi.fn()
const mockSignOut = vi.fn()
const mockGetSession = vi.fn()

vi.mock('@supabase/supabase-js', () => ({
  createClient: () => ({
    auth: {
      signInWithPassword: mockSignInWithPassword,
      signInWithOAuth: mockSignInWithOAuth,
      signOut: mockSignOut,
      getSession: mockGetSession,
      onAuthStateChange: vi.fn(() => ({
        data: { subscription: { unsubscribe: vi.fn() } },
      })),
    },
  }),
}))

import { useAuth } from '../useAuth'

describe('useAuth', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockGetSession.mockResolvedValue({ data: { session: null }, error: null })
  })

  it('exposes session and user refs as null initially', () => {
    const { session, user } = useAuth()
    expect(session.value).toBeNull()
    expect(user.value).toBeNull()
  })

  it('signInWithPassword calls supabase and sets session on success', async () => {
    const fakeSession = { access_token: 'jwt-token', user: { id: 'u1', email: 'deb@example.com' } }
    mockSignInWithPassword.mockResolvedValue({ data: { session: fakeSession }, error: null })

    const { signInWithPassword, session, user } = useAuth()
    await signInWithPassword('deb@example.com', 'password123')

    expect(mockSignInWithPassword).toHaveBeenCalledWith({ email: 'deb@example.com', password: 'password123' })
    expect(session.value).toEqual(fakeSession)
    expect(user.value).toEqual(fakeSession.user)
  })

  it('signInWithPassword throws on error', async () => {
    mockSignInWithPassword.mockResolvedValue({ data: {}, error: { message: 'Invalid credentials' } })

    const { signInWithPassword } = useAuth()
    await expect(signInWithPassword('deb@example.com', 'wrong')).rejects.toThrow('Invalid credentials')
  })

  it('signInWithGoogle calls supabase OAuth', async () => {
    mockSignInWithOAuth.mockResolvedValue({ data: {}, error: null })

    const { signInWithGoogle } = useAuth()
    await signInWithGoogle()

    expect(mockSignInWithOAuth).toHaveBeenCalledWith({ provider: 'google' })
  })

  it('signOut clears session and user', async () => {
    const fakeSession = { access_token: 'jwt-token', user: { id: 'u1', email: 'deb@example.com' } }
    mockSignInWithPassword.mockResolvedValue({ data: { session: fakeSession }, error: null })
    mockSignOut.mockResolvedValue({ error: null })

    const { signInWithPassword, signOut, session, user } = useAuth()
    await signInWithPassword('deb@example.com', 'password123')
    expect(session.value).not.toBeNull()

    await signOut()
    expect(mockSignOut).toHaveBeenCalled()
    expect(session.value).toBeNull()
    expect(user.value).toBeNull()
  })
})
