import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Index from '~/pages/index.vue'

beforeEach(() => {
  clearNuxtState()
  useState('session-loaded').value = true
})

const settle = () => flush(8)
const list = (...names: string[]) => names.map((name) => ({ name, kind: 'mk4', host: '10.0.0.5', username: 'maker' }))
const asRole = (role: string) => { useState('session').value = { username: 'kerry', role, csrf_token: 'x' } }

describe('dashboard', () => {
  it('shows the empty board with the admin hint for admins and the ask-an-admin line for viewers', async () => {
    stubFetch({ 'GET /api/printers': { body: [] } })
    asRole('admin')
    let w = await mountSuspended(Index)
    await settle()
    expect(w.text()).toContain('No printers yet')
    expect(w.text()).toContain('starstar printer add <name> --kind mk4 --host <address> --username <printer user>')
    asRole('viewer')
    w = await mountSuspended(Index)
    await settle()
    expect(w.text()).toContain('Ask an admin to add one.')
    expect(w.text()).not.toContain('starstar printer add')
  })

  it('summarises states and omits zero counts', async () => {
    stubFetch({
      'GET /api/printers': { body: list('a', 'b', 'c') },
      'GET /api/printers/a/status': { body: { name: 'a', state: 'PRINTING' } },
      'GET /api/printers/a/job': { status: 204 },
      'GET /api/printers/b/status': { body: { name: 'b', state: 'IDLE' } },
      'GET /api/printers/b/job': { status: 204 },
      'GET /api/printers/c/status': { status: 502, body: { error: 'x' } },
      'GET /api/printers/c/job': { status: 502, body: { error: 'x' } },
    })
    asRole('admin')
    const w = await mountSuspended(Index)
    await settle()
    expect(w.find('.summary').text()).toBe('1 printing · 1 idle · 1 not answering')
    expect(w.findAll('ul.board > li')).toHaveLength(3)
  })

  it('omits the summary counts that are zero', async () => {
    stubFetch({
      'GET /api/printers': { body: list('a') },
      'GET /api/printers/a/status': { body: { name: 'a', state: 'IDLE' } },
      'GET /api/printers/a/job': { status: 204 },
    })
    asRole('viewer')
    const w = await mountSuspended(Index)
    await settle()
    expect(w.find('.summary').text()).toBe('1 idle')
  })

  it('shows a list error with a retry button', async () => {
    stubFetch({ 'GET /api/printers': { status: 500, body: { error: 'database is down' } } })
    asRole('viewer')
    const w = await mountSuspended(Index)
    await settle()
    expect(w.text()).toContain('database is down')
    expect(w.text()).toContain('Try again')
  })
})
