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

async function save() {
  saving.value = true
  error.value = ''
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
