import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { stubFetch } from './helpers'
import PrinterCard from '~/components/PrinterCard.vue'
import type { PrinterSummary } from '~/types/api'

const printer = (name: string): PrinterSummary => ({ name, kind: 'mk4', host: '10.0.0.5', username: 'maker' })
const status = (name: string, state = 'PRINTING') => ({ name, state, nozzle_temp: 214.6, nozzle_target: 215, bed_temp: 60, bed_target: 60 })
const job = { id: 42, state: 'PRINTING', progress: 43.2, time_remaining: 4320, time_printing: 100, serial_print: false, file: { name: 'benchy.bgcode', display_name: 'Benchy.bgcode', path: '/usb/benchy.bgcode' } }

beforeEach(() => {
  clearNuxtState()
  vi.useFakeTimers()
})
afterEach(() => { vi.useRealTimers() })

const settle = async () => { await vi.advanceTimersByTimeAsync(0); await vi.advanceTimersByTimeAsync(0) }
async function mountCard(name: string) {
  const w = await mountSuspended(PrinterCard, { props: { printer: printer(name) } })
  await settle()
  return w
}

describe('PrinterCard', () => {
  it('renders state, temps and job when printing', async () => {
    stubFetch({ 'GET /api/printers/mk4/status': { body: status('mk4') }, 'GET /api/printers/mk4/job': { body: job } })
    const w = await mountCard('mk4')
    const t = w.text()
    expect(t).toContain('Printing')
    expect(t).toContain('215°')
    expect(t).toContain('43 %')
    expect(t).toContain('1 h 12 m left')
    expect(t).toContain('Benchy.bgcode')
    expect(w.classes()).not.toContain('is-shadow')
    expect(w.find('a').attributes('href')).toBe('/printers/mk4')
  })

  it('renders "No job" on 204', async () => {
    stubFetch({ 'GET /api/printers/mk4/status': { body: status('mk4', 'IDLE') }, 'GET /api/printers/mk4/job': { status: 204 } })
    const w = await mountCard('mk4')
    expect(w.text()).toContain('No job')
    expect(w.text()).toContain('Idle')
  })

  it('drops to the shadow on 502 and says when it retries', async () => {
    stubFetch({ 'GET /api/printers/mk4/status': { status: 502, body: { error: 'x' } }, 'GET /api/printers/mk4/job': { status: 502, body: { error: 'x' } } })
    const w = await mountCard('mk4')
    expect(w.classes()).toContain('is-shadow')
    expect(w.text()).toContain('Not answering')
    expect(w.text()).toContain('retrying in 10 s')
  })

  it('comes back filled after a success and shows last seen while failing', async () => {
    let up = true
    stubFetch({
      'GET /api/printers/mk4/status': () => (up ? { body: status('mk4') } : { status: 502, body: { error: 'x' } }),
      'GET /api/printers/mk4/job': () => (up ? { status: 204 } : { status: 502, body: { error: 'x' } }),
    })
    const w = await mountCard('mk4')
    expect(w.classes()).not.toContain('is-shadow')
    up = false
    await vi.advanceTimersByTimeAsync(5_000)
    expect(w.classes()).toContain('is-shadow')
    expect(w.text()).toContain('last seen')
    up = true
    await vi.advanceTimersByTimeAsync(10_000)
    expect(w.classes()).not.toContain('is-shadow')
  })

  it('one_failing_card_does_not_slow_others', async () => {
    const { calls } = stubFetch({
      'GET /api/printers/a/status': { status: 502, body: { error: 'x' } },
      'GET /api/printers/a/job': { status: 502, body: { error: 'x' } },
      'GET /api/printers/b/status': { body: status('b') },
      'GET /api/printers/b/job': { status: 204 },
    })
    await mountCard('a')
    await mountCard('b')
    await vi.advanceTimersByTimeAsync(5_000)
    const count = (n: string) => calls.filter((c) => c.path === `/api/printers/${n}/status`).length
    expect(count('b')).toBe(2)
    expect(count('a')).toBe(1)
  })
})
