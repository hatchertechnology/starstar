import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import CameraSnapshot from '~/components/CameraSnapshot.vue'

let n = 0
const revoke = vi.fn()
beforeEach(() => {
  clearNuxtState()
  useState('session-loaded').value = true
  revoke.mockReset()
  n = 0
  URL.createObjectURL = vi.fn(() => `blob:snap-${++n}`)
  URL.revokeObjectURL = revoke
})

const png = () => ({ raw: new Uint8Array([1, 2, 3]), headers: { 'content-type': 'image/png' } })
const button = (w: Awaited<ReturnType<typeof mountSuspended>>, label: string) => w.findAll('button').find((b) => b.text() === label)!

describe('CameraSnapshot', () => {
  it('shows the image with alt text and revokes the old object URL on refresh and unmount', async () => {
    stubFetch({ 'GET /api/printers/mk4/cameras/snap': png })
    const w = await mountSuspended(CameraSnapshot, { props: { printer: 'mk4' } })
    await flush()
    const img = w.find('img')
    expect(img.attributes('src')).toBe('blob:snap-1')
    expect(img.attributes('alt')).toBe('Latest camera snapshot of mk4')
    expect(w.text()).toContain('Taken just now')
    await button(w, 'Refresh snapshot').trigger('click')
    await flush()
    expect(revoke).toHaveBeenCalledWith('blob:snap-1')
    expect(w.find('img').attributes('src')).toBe('blob:snap-2')
    w.unmount()
    expect(revoke).toHaveBeenCalledWith('blob:snap-2')
  })

  it('handles 204', async () => {
    stubFetch({ 'GET /api/printers/mk4/cameras/snap': { status: 204 } })
    const w = await mountSuspended(CameraSnapshot, { props: { printer: 'mk4' } })
    await flush()
    expect(w.text()).toContain('No snapshot yet. The camera takes them while printing.')
    expect(w.find('img').exists()).toBe(false)
  })

  it('shows the error and a retry button', async () => {
    stubFetch({ 'GET /api/printers/mk4/cameras/snap': { status: 502, body: { error: 'camera offline' } } })
    const w = await mountSuspended(CameraSnapshot, { props: { printer: 'mk4' } })
    await flush()
    expect(w.text()).toContain('camera offline')
    expect(button(w, 'Try again').exists()).toBe(true)
  })
})
