import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

const mockRequest = vi.fn()
vi.mock('../../composables/useApi', () => ({
  useApi: () => ({
    request: mockRequest,
    loading: { value: false },
    error: { value: null },
    setToken: vi.fn(),
    clearToken: vi.fn(),
  }),
  ApiError: class extends Error {
    code: string
    status: number
    constructor(status: number, code: string, message: string) {
      super(message)
      this.name = 'ApiError'
      this.code = code
      this.status = status
    }
  },
}))

vi.mock('../../composables/useAuth', () => ({
  useAuth: () => ({
    session: { value: { access_token: 'jwt' } },
    user: { value: { email: 'test@example.com' } },
    ready: { value: true },
  }),
}))

import Game from '../../components/Game.vue'
import Wallet from '../../components/Wallet.vue'
import Stats from '../../components/Stats.vue'
import { ApiError } from '../../composables/useApi'

function mockChallenge() {
  return {
    challenge_id: 'abc-123',
    problem: '7 + 3',
    difficulty: 3,
    reward: 20,
    expires_at: new Date(Date.now() + 60_000).toISOString(),
  }
}

describe('Game', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    // clear intervals from countdown
  })

  it('fetches challenge on mount and displays problem', async () => {
    mockRequest.mockResolvedValueOnce(mockChallenge())
    const wrapper = mount(Game)
    await flushPromises()

    expect(mockRequest).toHaveBeenCalledWith('/api/challenge')
    expect(wrapper.text()).toContain('7 + 3')
  })

  it('submits correct answer and shows success', async () => {
    mockRequest.mockResolvedValueOnce(mockChallenge())
    mockRequest.mockResolvedValueOnce({ status: 'CLAIMED', reward: 20, balance: 20 })

    const wrapper = mount(Game)
    await flushPromises()

    await wrapper.find('input').setValue('10')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Correct')
  })

  it('shows error on wrong answer and allows new challenge', async () => {
    mockRequest.mockResolvedValueOnce(mockChallenge())
    mockRequest.mockRejectedValueOnce(new ApiError(422, 'incorrect_solution', 'incorrect'))

    const wrapper = mount(Game)
    await flushPromises()

    await wrapper.find('input').setValue('99')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Wrong')
    expect(wrapper.text()).toContain('Next challenge')
  })

  it('shows rate-limited message on 429', async () => {
    mockRequest.mockResolvedValueOnce(mockChallenge())
    mockRequest.mockRejectedValueOnce(new ApiError(429, 'rate_limited', 'Too many'))

    const wrapper = mount(Game)
    await flushPromises()

    await wrapper.find('input').setValue('10')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Slow down')
  })
})

describe('Wallet', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('displays balance from API', async () => {
    mockRequest.mockResolvedValueOnce({ email: 'test@example.com', balance: 120, total_mined: 6 })
    const wrapper = mount(Wallet)
    await flushPromises()

    expect(mockRequest).toHaveBeenCalledWith('/api/me')
    expect(wrapper.text()).toContain('120')
  })

  it('exposes a refresh method that re-fetches', async () => {
    mockRequest.mockResolvedValueOnce({ email: 'test@example.com', balance: 10, total_mined: 1 })
    mockRequest.mockResolvedValueOnce({ email: 'test@example.com', balance: 30, total_mined: 2 })

    const wrapper = mount(Wallet)
    await flushPromises()
    expect(wrapper.text()).toContain('10')

    await wrapper.vm.refresh()
    await flushPromises()
    expect(wrapper.text()).toContain('30')
  })
})

describe('Stats', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders difficulty, rate, and supply from API', async () => {
    mockRequest.mockResolvedValueOnce({
      current_difficulty: 5,
      mints_last_60s: 18,
      total_accrued_supply: 4200,
    })

    const wrapper = mount(Stats)
    await flushPromises()

    expect(wrapper.text()).toContain('5')
    expect(wrapper.text()).toContain('18')
    expect(wrapper.text()).toContain('4200')
  })
})
