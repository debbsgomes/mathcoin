import { ref, onUnmounted } from 'vue'
import { createClient, type SupabaseClient, type Session, type User } from '@supabase/supabase-js'

let _supabase: SupabaseClient | null = null

function getSupabase(): SupabaseClient {
  if (!_supabase) {
    const supabaseUrl = import.meta.env.VITE_SUPABASE_URL || 'https://placeholder.supabase.co'
    const supabaseAnonKey = import.meta.env.VITE_SUPABASE_ANON_KEY || 'placeholder-key'
    _supabase = createClient(supabaseUrl, supabaseAnonKey)
  }
  return _supabase
}

// Module-level shared init promise — all useAuth() instances share the same session restore
let _initPromise: Promise<void> | null = null
// Shared reactive state so multiple components observe the same session
let _sharedSession: ReturnType<typeof ref<Session | null>> | null = null
let _sharedUser: ReturnType<typeof ref<User | null>> | null = null
let _sharedReady = ref(false)
let _subscribed = false

export function useAuth() {
  const supabase = getSupabase()

  // Lazy-init shared state on first call
  if (!_initPromise) {
    _sharedSession = ref<Session | null>(null)
    _sharedUser = ref<User | null>(null)

    _initPromise = supabase.auth.getSession().then(({ data }) => {
      _sharedSession!.value = data.session ?? null
      _sharedUser!.value = data.session?.user ?? null
      _sharedReady.value = true
    })
  }

  // Listen for auth state changes (once, module-level)
  if (!_subscribed) {
    _subscribed = true
    const { data: sub } = supabase.auth.onAuthStateChange((_event, newSession) => {
      _sharedSession!.value = newSession
      _sharedUser!.value = newSession?.user ?? null
      _sharedReady.value = true
    })
    onUnmounted(() => {
      sub?.subscription?.unsubscribe()
    })
  }

  async function signInWithPassword(email: string, password: string) {
    const { data, error } = await supabase.auth.signInWithPassword({ email, password })
    if (error) throw new Error(error.message)
    _sharedSession!.value = data.session
    _sharedUser!.value = data.session?.user ?? null
  }

  async function signInWithGoogle() {
    const { error } = await supabase.auth.signInWithOAuth({ provider: 'google' })
    if (error) throw new Error(error.message)
  }

  async function signOut() {
    await supabase.auth.signOut()
    _sharedSession!.value = null
    _sharedUser!.value = null
  }

  return {
    session: _sharedSession!,
    user: _sharedUser!,
    ready: _sharedReady,
    signInWithPassword,
    signInWithGoogle,
    signOut,
  }
}
