<template>
  <div class="audit card">
    <h3>Proof of Reserves</h3>
    <div v-if="!onchainEnabled">
      <p class="muted">On-chain features are not enabled in this environment.</p>
    </div>
    <div v-else>
      <p>Contract: <a :href="explorerLink" target="_blank">{{ contractAddress }}</a></p>
      <p>Chain: {{ chain }}</p>
      <p v-if="merkleRoot">Merkle Root: {{ merkleRoot }}</p>
      <p>Total Supply: {{ totalSupply }} MATH</p>
      <p>Distributions: {{ distCount }}</p>
      <p v-if="lastPublished">Last published: {{ lastPublished }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useApi } from '../composables/useApi'

const api = useApi()
const onchainEnabled = ref(false)
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
    onchainEnabled.value = data.onchain_enabled ?? false
    contractAddress.value = data.contract_address ?? ''
    chain.value = data.chain ?? ''
    explorer.value = data.explorer ?? ''
    merkleRoot.value = data.merkle_root ?? null
    totalSupply.value = data.total_accrued_supply ?? 0
    distCount.value = data.distribution_count ?? 0
    lastPublished.value = data.last_published_at ?? null
  } catch { /* audit is public; silent fail */ }
})
</script>
