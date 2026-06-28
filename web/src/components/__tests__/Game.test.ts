import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

// Mock useApi
const mockRequest = vi.fn()
vi.mock('../../composables/useApi', () => ({
  useApi: () => ({
    request: mockRequest,
    loading: { value: false },
    error: { value: null },
    setToken: vi.fn(),
    clearToken: vi.fn(),
  }),
}))

// Mock useAuth
vi.mock('../../composables/useAuth', () => ({
  useAuth: () => ({
    session: { value: { access_token: 'jwt' } },
    user: { value: { email: 'test@example.com' } },
    ready: { value: true },
  }),
}))

import Game from '../../components/Game.vue'
import Wallet from '../../components/Wallet.vue'

describe('Game', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('fetches challenge on mount and displays problem', async () => {
    mockRequest.mockResolvedValueOnce({
      challenge_id: 'abc-123',
      problem: '7 + 3',
      difficulty: 3,
      reward: 20,
      expires_at: '2026-01-01T00:00:00Z',
    })

    const wrapper = mount(Game)
    await flushPromises()

    expect(mockRequest).toHaveBeenCalledWith('/api/challenge')
    expect(wrapper.text()).toContain('7 + 3')
    expect(wrapper.find('input').exists()).toBe(true)
    expect(wrapper.find('button').text()).toContain('MINE')
  })

  it('submits correct answer and shows success', async () => {
    mockRequest.mockResolvedValueOnce({
      challenge_id: 'abc-123',
      problem: '7 + 3',
      difficulty: 3,
      reward: 20,
      expires_at: '2026-01-01T00:00:00Z',
    })
    mockRequest.mockResolvedValueOnce({
      status: 'CLAIMED',
      reward: 20,
      balance: 20,
    })

    const wrapper = mount(Game)
    await flushPromises()

    await wrapper.find('input').setValue('10')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(mockRequest).toHaveBeenCalledWith('/api/mint', expect.objectContaining({
      method: 'POST',
    }))
    expect(wrapper.text()).toContain('Correct')
  })

  it('shows error on wrong answer and allows new challenge', async () => {
    mockRequest.mockResolvedValueOnce({
      challenge_id: 'abc-123',
      problem: '7 + 3',
      difficulty: 3,
      reward: 20,
      expires_at: '2026-01-01T00:00:00Z',
    })
    const err = new Error('incorrect solution')
    mockRequest.mockRejectedValueOnce(err)

    const wrapper = mount(Game)
    await flushPromises()

    await wrapper.find('input').setValue('99')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Wrong')
    // Should show a "Next challenge" button
    expect(wrapper.find('button').text()).toContain('Next')
  })

  it('shows loading while fetching challenge', async () => {
    mockRequest.mockReturnValueOnce(new Promise(() => {})) // never resolves

    const wrapper = mount(Game)
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('Loading challenge')
  })
})

describe('Wallet', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('displays balance from API', async () => {
    mockRequest.mockResolvedValueOnce({
      email: 'test@example.com',
      balance: 120,
      total_mined: 6,
      claim_address: null,
    })

    const wrapper = mount(Wallet)
    await flushPromises()

    expect(mockRequest).toHaveBeenCalledWith('/api/me')
    expect(wrapper.text()).toContain('120')
  })

  it('exposes a refresh method that re-fetches', async () => {
    mockRequest.mockResolvedValueOnce({
      email: 'test@example.com',
      balance: 10,
      total_mined: 1,
    })
    mockRequest.mockResolvedValueOnce({
      email: 'test@example.com',
      balance: 30,
      total_mined: 2,
    })

    const wrapper = mount(Wallet)
    await flushPromises()
    expect(wrapper.text()).toContain('10')

    await wrapper.vm.refresh()
    await flushPromises()
    expect(wrapper.text()).toContain('30')
  })
})
