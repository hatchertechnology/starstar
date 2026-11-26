import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import Index from '~/pages/index.vue'

describe('smoke', () => {
  it('renders the Printers heading', async () => {
    const wrapper = await mountSuspended(Index)
    expect(wrapper.text()).toContain('Printers')
  })
})
