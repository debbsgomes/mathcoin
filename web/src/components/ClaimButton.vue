<template>
  <div>
    <div v-if="state === 'idle'">
      <button @click="claim">Send to chain</button>
    </div>
    <div v-else-if="state === 'submitting'">
      <p>Submitting...</p>
    </div>
    <div v-else-if="state === 'submitted'">
      <p class="success">Claim submitted! Tx: {{ txHash }}</p>
    </div>
    <div v-else-if="state === 'syncing'">
      <p>No published distribution yet. Available after the next sync (~1h).</p>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useApi, ApiError } from '../../composables/useApi'

const props = defineProps<{ address: string }>()

const api = useApi()
const state = ref<'idle' | 'submitting' | 'submitted' | 'syncing'>('idle')
const txHash = ref('')
const error = ref('')

async function claim() {
  state.value = 'submitting'
  error.value = ''
  try {
    const data = await api.request('/api/claim-relay', { method: 'POST' })
    txHash.value = data.tx_hash
    state.value = 'submitted'
  } catch (e: any) {
    if (e instanceof ApiError) {
      state.value = 'syncing'
    } else {
      error.value = e.message || 'Claim failed'
      state.value = 'idle'
    }
  }
}
</script>
