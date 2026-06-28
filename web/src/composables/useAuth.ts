import { ref, onUnmounted } from 'vue'
import { createClient, type SupabaseClient } from '@supabase/supabase-js'

let _supabase: SupabaseClient | null = null

function getSupabase(): SupabaseClient {
  if (!_supabase) {
    const supabaseUrl = import.meta.env.VITE_SUPABASE_URL || 'https://placeholder.supabase.co'
    const supabaseAnonKey = import.meta.env.VITE_SUPABASE_ANON_KEY || 'placeholder-key'
    _supabase = createClient(supabaseUrl, supabaseAnonKey)
  }
  return _supabase
}

let _initPromise: Promise<void> | null = null

export function useAuth() {
  const supabase = getSupabase()
  const session = ref<any>(null)
  const user = ref<any>(null)
  const ready = ref(false)

  // Lazy-init: restore session from Supabase's local storage on first call
  if (!_initPromise) {
    _initPromise = supabase.auth.getSession().then(({ data }) => {
      session.value = data.session ?? null
      user.value = data.session?.user ?? null
      ready.value = true
    })
  } else {
    _initPromise.then(() => {
      ready.value = true
    })
  }

  // Listen for auth state changes
  const { data: sub } = supabase.auth.onAuthStateChange((_event, newSession) => {
    session.value = newSession
    user.value = newSession?.user ?? null
  })

  onUnmounted(() => {
    sub?.subscription?.unsubscribe()
  })

  async function signInWithPassword(email: string, password: string) {
    const { data, error } = await supabase.auth.signInWithPassword({ email, password })
    if (error) throw new Error(error.message)
    session.value = data.session
    user.value = data.session?.user ?? null
  }

  async function signInWithGoogle() {
    const { error } = await supabase.auth.signInWithOAuth({ provider: 'google' })
    if (error) throw new Error(error.message)
  }

  async function signOut() {
    await supabase.auth.signOut()
    session.value = null
    user.value = null
  }

  return { session, user, ready, signInWithPassword, signInWithGoogle, signOut }
}
