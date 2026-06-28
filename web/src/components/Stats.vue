<template>
  <div class="stats card">
    <p>Difficulty: <strong>{{ difficulty }}</strong> · Mints/60s: <strong>{{ rate }}</strong> · Supply: <strong>{{ supply }}</strong></p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useApi } from '../composables/useApi'

const api = useApi()
const difficulty = ref(0)
const rate = ref(0)
const supply = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

async function fetchStats() {
  try {
    const data = await api.request('/api/stats')
    difficulty.value = data.current_difficulty ?? 0
    rate.value = Math.round(data.mints_last_60s ?? 0)
    supply.value = data.total_accrued_supply ?? 0
  } catch {
    // stats are non-critical; silently ignore
  }
}

onMounted(() => {
  fetchStats()
  timer = setInterval(fetchStats, 10_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
