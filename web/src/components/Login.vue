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
        <Wallet ref="walletRef" />
        <Game @minted="onMinted" />
        <Stats />
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
import Game from './Game.vue'
import Wallet from './Wallet.vue'
import Stats from './Stats.vue'

const { session, user, ready, signInWithPassword, signInWithGoogle, signOut } = useAuth()
const api = useApi()

const email = ref('')
const password = ref('')
const error = ref('')
const walletRef = ref<InstanceType<typeof Wallet> | null>(null)

watch(session, (newSession) => {
  if (newSession?.access_token) {
    api.setToken(newSession.access_token)
    error.value = ''
  } else {
    api.clearToken()
  }
})

function onMinted() {
  walletRef.value?.refresh()
}

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
