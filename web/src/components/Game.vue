<template>
  <div class="game card">
    <div v-if="loading">
      <p>Loading challenge...</p>
    </div>
    <div v-else-if="challenge">
      <p class="problem">{{ challenge.problem }}</p>
      <p class="meta">Difficulty {{ challenge.difficulty }} · Reward {{ challenge.reward }} MATH · Expires {{ countdown }}s</p>
      <form v-if="!result" @submit.prevent="handleMint">
        <input v-model="answer" type="number" placeholder="Answer" autofocus :disabled="submitting" />
        <button type="submit" :disabled="submitting">[ MINE! ]</button>
      </form>
      <div v-else>
        <p :class="resultClass">{{ resultMessage }}</p>
        <button v-if="result !== 'correct'" @click="fetchChallenge">Next challenge</button>
      </div>
      <p v-if="rateLimited" class="error">Slow down! Too many requests.</p>
    </div>
    <p v-if="error && !result" class="error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useApi, ApiError } from '../composables/useApi'

const emit = defineEmits<{ minted: [] }>()

const api = useApi()
const challenge = ref<{ challenge_id: string; problem: string; difficulty: number; reward: number; expires_at: string } | null>(null)
const answer = ref('')
const result = ref<'correct' | 'wrong' | 'expired' | null>(null)
const resultMessage = ref('')
const resultClass = ref('')
const loading = ref(false)
const submitting = ref(false)
const error = ref('')
const rateLimited = ref(false)
const countdown = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

function startCountdown(expiresAt: string) {
  stopCountdown()
  const tick = () => {
    const remaining = Math.max(0, Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000))
    countdown.value = remaining
    if (remaining <= 0) {
      stopCountdown()
      result.value = 'expired'
      resultMessage.value = 'Challenge expired!'
      resultClass.value = 'fail'
    }
  }
  tick()
  timer = setInterval(tick, 1000)
}

function stopCountdown() {
  if (timer) { clearInterval(timer); timer = null }
}

async function fetchChallenge() {
  loading.value = true
  error.value = ''
  result.value = null
  answer.value = ''
  rateLimited.value = false
  stopCountdown()
  try {
    challenge.value = await api.request('/api/challenge')
    if (challenge.value?.expires_at) {
      startCountdown(challenge.value.expires_at)
    }
  } catch (e: any) {
    if (e instanceof ApiError && e.status === 429) {
      rateLimited.value = true
    } else {
      error.value = e.message || 'Failed to load challenge'
    }
  } finally {
    loading.value = false
  }
}

async function handleMint() {
  if (!challenge.value) return
  submitting.value = true
  error.value = ''
  rateLimited.value = false
  try {
    const data = await api.request('/api/mint', {
      method: 'POST',
      body: JSON.stringify({ challenge_id: challenge.value.challenge_id, answer: parseInt(answer.value) }),
    })
    result.value = 'correct'
    resultMessage.value = `Correct! +${data.reward} MATH`
    resultClass.value = 'success'
    stopCountdown()
    emit('minted')
  } catch (e: any) {
    if (e instanceof ApiError) {
      if (e.code === 'incorrect_solution') {
        result.value = 'wrong'
        resultMessage.value = 'Wrong answer!'
        resultClass.value = 'fail'
      } else if (e.code === 'challenge_expired') {
        result.value = 'expired'
        resultMessage.value = 'Challenge expired!'
        resultClass.value = 'fail'
      } else if (e.code === 'challenge_already_resolved') {
        result.value = 'expired'
        resultMessage.value = 'Already claimed!'
        resultClass.value = 'fail'
      } else if (e.status === 429) {
        rateLimited.value = true
      } else {
        error.value = e.message
      }
    } else {
      error.value = e.message || 'Network error'
    }
  } finally {
    submitting.value = false
  }
}

onMounted(fetchChallenge)
onUnmounted(stopCountdown)
</script>
