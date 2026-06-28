<template>
  <div class="game">
    <div v-if="loading">
      <p>Loading challenge...</p>
    </div>
    <div v-else-if="challenge">
      <p class="problem">{{ challenge.problem }}</p>
      <p class="meta">Difficulty {{ challenge.difficulty }} · Reward {{ challenge.reward }} MATH</p>
      <form v-if="!result" @submit.prevent="handleMint">
        <input
          v-model="answer"
          type="number"
          placeholder="Your answer"
          autofocus
          :disabled="submitting"
        />
        <button type="submit" :disabled="submitting">[ MINE! ]</button>
      </form>
      <div v-else>
        <p :class="resultClass">{{ resultMessage }}</p>
        <button v-if="result !== 'correct'" @click="fetchChallenge">Next challenge</button>
      </div>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useApi, ApiError } from '../composables/useApi'

const emit = defineEmits<{ minted: [] }>()

const api = useApi()
const challenge = ref<{ challenge_id: string; problem: string; difficulty: number; reward: number } | null>(null)
const answer = ref('')
const result = ref<'correct' | 'wrong' | 'expired' | null>(null)
const resultMessage = ref('')
const resultClass = ref('')
const loading = ref(false)
const submitting = ref(false)
const error = ref('')

async function fetchChallenge() {
  loading.value = true
  error.value = ''
  result.value = null
  answer.value = ''
  try {
    challenge.value = await api.request('/api/challenge')
  } catch (e: any) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

async function handleMint() {
  if (!challenge.value) return
  submitting.value = true
  error.value = ''
  try {
    const data = await api.request('/api/mint', {
      method: 'POST',
      body: JSON.stringify({
        challenge_id: challenge.value.challenge_id,
        answer: parseInt(answer.value),
      }),
    })
    result.value = 'correct'
    resultMessage.value = `Correct! +${data.reward} MATH`
    resultClass.value = 'success'
    emit('minted')
  } catch (e: any) {
    // Match on structured error codes from the backend JSON envelope
    if (e instanceof ApiError) {
      if (e.code === 'incorrect_solution') {
        result.value = 'wrong'
        resultMessage.value = 'Wrong answer!'
        resultClass.value = 'fail'
        error.value = ''
      } else if (e.code === 'challenge_expired') {
        result.value = 'expired'
        resultMessage.value = 'Challenge expired!'
        resultClass.value = 'fail'
        error.value = ''
      } else if (e.code === 'challenge_already_resolved') {
        result.value = 'expired'
        resultMessage.value = 'Already claimed!'
        resultClass.value = 'fail'
        error.value = ''
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
</script>
