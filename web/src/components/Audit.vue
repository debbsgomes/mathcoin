<template>
  <div class="audit card">
    <h3>Proof of Reserves</h3>
    <p>Contract: <a :href="explorerLink" target="_blank">{{ contractAddress }}</a></p>
    <p>Chain: {{ chain }}</p>
    <p v-if="merkleRoot">Merkle Root: {{ merkleRoot }}</p>
    <p>Total Supply: {{ totalSupply }} MATH</p>
    <p>Distributions: {{ distCount }}</p>
    <p v-if="lastPublished">Last published: {{ lastPublished }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useApi } from '../composables/useApi'

const api = useApi()
const contractAddress = ref('')
const chain = ref('')
const explorer = ref('')
const merkleRoot = ref<string | null>(null)
const totalSupply = ref(0)
const distCount = ref(0)
const lastPublished = ref<string | null>(null)

const explorerLink = computed(() => `${explorer.value}/address/${contractAddress.value}`)

onMounted(async () => {
  try {
    const data = await api.request('/api/audit')
    contractAddress.value = data.contract_address
    chain.value = data.chain
    explorer.value = data.explorer
    merkleRoot.value = data.merkle_root
    totalSupply.value = data.total_accrued_supply
    distCount.value = data.distribution_count
    lastPublished.value = data.last_published_at
  } catch { /* audit is public; silent fail */ }
})
</script>
