import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import PrinterPage from '~/pages/printers/[name]/index.vue'

beforeEach(() => {
  clearNuxtState()
  useState('session').value = { username: 'u', role: 'operator', csrf_token: 'x' }
  useState('session-loaded').value = true
})

describe('printer detail page', () => {
  it('says so when the printer does not exist', async () => {
    stubFetch({ 'GET /api/printers/ghost': { status: 404, body: { error: 'no such printer' } } })
    const w = await mountSuspended(PrinterPage, { route: '/printers/ghost' })
    await flush(8)
    expect(w.text()).toContain('No printer called ghost.')
    w.unmount()
  })

  it('shows the header, state and controls for a printing job', async () => {
    stubFetch({
      'GET /api/printers/mk4': { body: { name: 'mk4', kind: 'mk4', host: '10.0.0.5', username: 'maker' } },
      'GET /api/printers/mk4/status': { body: { name: 'mk4', state: 'PRINTING', nozzle_temp: 215, nozzle_target: 215, bed_temp: 60, bed_target: 60 } },
      'GET /api/printers/mk4/job': { body: { id: 42, state: 'PRINTING', progress: 10, time_printing: 1, serial_print: false, file: { name: 'a.bgcode', path: '/usb/a.bgcode' } } },
      'GET /api/printers/mk4/cameras/snap': { status: 204 },
      'GET /api/printers/mk4/storage': { body: [] },
    })
    const w = await mountSuspended(PrinterPage, { route: '/printers/mk4' })
    await flush(10)
    expect(w.text()).toContain('MK4 · 10.0.0.5')
    expect(w.text()).toContain('Printing')
    expect(w.findAll('.row button').map((b) => b.text())).toEqual(['Pause', 'Stop'])
    w.unmount()
  })
})
