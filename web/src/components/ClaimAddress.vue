<template>
  <div class="claim card">
    <h3>Send Coins On-Chain</h3>
    <div v-if="!address">
      <p>Enter your Base address to claim your MATH tokens on-chain.</p>
      <form @submit.prevent="save">
        <input v-model="input" placeholder="0x..." :disabled="saving" />
        <button type="submit" :disabled="saving">Save Address</button>
      </form>
      <p v-if="error" class="error">{{ error }}</p>
    </div>
    <div v-else>
      <p>Address: {{ address }}</p>
      <ClaimButton :address="address" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useApi } from '../../composables/useApi'
import { defineAsyncComponent } from 'vue'

const ClaimButton = defineAsyncComponent(() => import('./ClaimButton.vue'))

const api = useApi()
const address = ref('')
const input = ref('')
const saving = ref(false)
const error = ref('')

const ETH_ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/

async function save() {
  saving.value = true
  error.value = ''

  if (!ETH_ADDRESS_RE.test(input.value)) {
    error.value = 'Invalid Ethereum address. Must be 0x followed by 40 hex characters.'
    saving.value = false
    return
  }

  try {
    const data = await api.request('/api/claim-address', {
      method: 'POST',
      body: JSON.stringify({ address: input.value }),
    })
    address.value = data.claim_address
  } catch (e: any) {
    error.value = e.message || 'Failed to save address'
  } finally {
    saving.value = false
  }
}
</script>
