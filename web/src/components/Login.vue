<template>
  <div class="login">
    <h1>MathCoin</h1>
    <div v-if="!user">
      <form @submit.prevent="handleEmailSignIn">
        <input v-model="email" type="email" placeholder="Email" required />
        <input v-model="password" type="password" placeholder="Password" required />
        <button type="submit">Sign in with Email</button>
      </form>
      <button @click="handleGoogleSignIn">Sign in with Google</button>
    </div>
    <div v-else>
      <p>Logged in as {{ user.email }}</p>
      <p>Balance: {{ balance }} MATH</p>
      <button @click="handleSignOut">Sign out</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useAuth } from '../composables/useAuth'
import { useApi } from '../composables/useApi'

const { session, user, signInWithPassword, signInWithGoogle, signOut } = useAuth()
const api = useApi()

const email = ref('')
const password = ref('')
const balance = ref(0)

async function handleEmailSignIn() {
  await signInWithPassword(email.value, password.value)
  await createSession()
}

async function handleGoogleSignIn() {
  await signInWithGoogle()
}

async function handleSignOut() {
  await signOut()
  api.clearToken()
}

async function createSession() {
  if (session.value?.access_token) {
    api.setToken(session.value.access_token)
    try {
      const data = await api.request('/api/session', { method: 'POST' })
      balance.value = data.balance
    } catch {
      // session endpoint will be called on next action
    }
  }
}

onMounted(async () => {
  if (session.value?.access_token) {
    await createSession()
  }
})
</script>
