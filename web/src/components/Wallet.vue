<template>
  <div class="wallet">
    <p>Balance: <strong>{{ balance }}</strong> MATH</p>
    <p class="sub">Mined {{ totalMined }} times</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useApi } from '../composables/useApi'

const api = useApi()
const balance = ref(0)
const totalMined = ref(0)

async function fetchMe() {
  try {
    const data = await api.request('/api/me')
    balance.value = data.balance ?? 0
    totalMined.value = data.total_mined ?? 0
  } catch {
    // silently ignore errors on balance fetch
  }
}

function refresh() {
  fetchMe()
}

defineExpose({ refresh })

onMounted(fetchMe)
</script>
