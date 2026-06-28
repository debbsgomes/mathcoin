<template>
  <div class="login">
    <h1>MathCoin</h1>
    <div v-if="ready">
      <div v-if="!user">
        <form @submit.prevent="handleEmailSignIn">
          <input v-model="email" type="email" placeholder="Email" required />
          <input v-model="password" type="password" placeholder="Password" required />
          <button type="submit">Sign in with Email</button>
        </form>
        <button @click="handleGoogleSignIn">Sign in with Google</button>
        <p v-if="error" class="error">{{ error }}</p>
      </div>
      <div v-else>
        <p>Logged in as {{ user.email }}</p>
        <p>Balance: {{ balance }} MATH</p>
        <button @click="handleSignOut">Sign out</button>
      </div>
    </div>
    <div v-else>
      <p>Loading...</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useAuth } from '../composables/useAuth'
import { useApi } from '../composables/useApi'

const { session, user, ready, signInWithPassword, signInWithGoogle, signOut } = useAuth()
const api = useApi()

const email = ref('')
const password = ref('')
const balance = ref(0)
const error = ref('')

// When session appears (login or restore), call POST /api/session then GET /api/me
watch(session, async (newSession) => {
  if (newSession?.access_token) {
    api.setToken(newSession.access_token)
    try {
      await api.request('/api/session', { method: 'POST' })
      const me = await api.request('/api/me')
      balance.value = me.balance ?? 0
      error.value = ''
    } catch (e: any) {
      error.value = e.message
    }
  } else {
    api.clearToken()
    balance.value = 0
  }
})

async function handleEmailSignIn() {
  try {
    await signInWithPassword(email.value, password.value)
  } catch (e: any) {
    error.value = e.message
  }
}

async function handleGoogleSignIn() {
  await signInWithGoogle()
}

async function handleSignOut() {
  await signOut()
}
</script>
