import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import JobControls from '~/components/JobControls.vue'
import type { Job, Role } from '~/types/api'

const job = (state = 'PRINTING'): Job => ({ id: 42, state, progress: 43.2, time_printing: 10, serial_print: false, file: { name: 'benchy.bgcode', display_name: 'Benchy.bgcode', path: '/usb/benchy.bgcode' } })

beforeEach(() => {
  clearNuxtState()
  useState('session').value = { username: 'op', role: 'operator', csrf_token: 'ss_cs_x' }
  useState('session-loaded').value = true
})

const mount = (o: { role?: Role; state?: string; job?: Job | null } = {}) =>
  mountSuspended(JobControls, { props: { printer: 'mk4', printerState: o.state ?? 'PRINTING', job: o.job === undefined ? job(o.state) : o.job, role: o.role ?? 'operator' }, attachTo: document.body })
const button = (w: Awaited<ReturnType<typeof mount>>, label: string) => w.findAll('button').find((b) => b.text() === label)!

describe('JobControls', () => {
  it('viewer sees no controls and the operator note', async () => {
    const w = await mount({ role: 'viewer' })
    expect(w.findAll('.row button')).toHaveLength(0)
    expect(w.text()).toContain('Operators can pause and stop prints.')
    w.unmount()
  })

  it('operator pausing a printing job PUTs /api/printers/mk4/job/42/pause with csrf and emits changed', async () => {
    const { calls } = stubFetch({ 'PUT /api/printers/mk4/job/42/pause': { status: 204 } })
    const w = await mount()
    await button(w, 'Pause').trigger('click')
    await flush()
    expect(calls).toHaveLength(1)
    expect(calls[0]!.headers.get('x-csrf-token')).toBe('ss_cs_x')
    expect(w.emitted('changed')).toHaveLength(1)
    w.unmount()
  })

  it('shows Resume for paused and Continue for attention', async () => {
    let w = await mount({ state: 'PAUSED' })
    expect(w.findAll('.row button').map((b) => b.text())).toEqual(['Resume', 'Stop'])
    w.unmount()
    w = await mount({ state: 'ATTENTION' })
    expect(w.findAll('.row button').map((b) => b.text())).toEqual(['Continue', 'Stop'])
    w.unmount()
  })

  it('stop asks first and names printer and file', async () => {
    const { calls } = stubFetch({ 'DELETE /api/printers/mk4/job/42': { status: 204 } })
    const w = await mount()
    await button(w, 'Stop').trigger('click')
    expect(w.text()).toContain('Stop the print on mk4?')
    expect(w.text()).toContain("Benchy.bgcode is 43 % done. A stopped print can't be resumed.")
    expect(calls).toHaveLength(0)
    await button(w, 'Stop print').trigger('click')
    await flush()
    expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual(['DELETE /api/printers/mk4/job/42'])
    expect(w.emitted('changed')).toHaveLength(1)
    w.unmount()
  })

  it('stop cancel sends nothing', async () => {
    const { calls } = stubFetch({})
    const w = await mount()
    await button(w, 'Stop').trigger('click')
    await button(w, 'Cancel').trigger('click')
    expect(calls).toHaveLength(0)
    expect((w.find('dialog').element as HTMLDialogElement).open).toBe(false)
    w.unmount()
  })

  it('a 409 on stop stays in the dialog with the message', async () => {
    stubFetch({ 'DELETE /api/printers/mk4/job/42': { status: 409, body: { error: 'job already finished' } } })
    const w = await mount()
    await button(w, 'Stop').trigger('click')
    await button(w, 'Stop print').trigger('click')
    await flush()
    expect((w.find('dialog').element as HTMLDialogElement).open).toBe(true)
    expect(w.find('dialog').text()).toContain('job already finished')
    expect(w.emitted('changed')).toBeUndefined()
    w.unmount()
  })

  it('a failed pause shows the message inline', async () => {
    stubFetch({ 'PUT /api/printers/mk4/job/42/pause': { status: 409, body: { error: 'cannot pause now' } } })
    const w = await mount()
    await button(w, 'Pause').trigger('click')
    await flush()
    expect(w.text()).toContain('cannot pause now')
    w.unmount()
  })
})
