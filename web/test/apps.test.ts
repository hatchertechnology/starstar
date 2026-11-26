import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Apps from '~/pages/apps.vue'

const grant = (o: Record<string, unknown> = {}) => ({ id: 'ss_gr_1', client_id: 'ss_cl_abc', client_name: 'Claude Code', username: 'kerry', role: 'operator', resource: null, created_at: 1764115200, last_used_at: null, expires_at: 1795651200, ...o })

beforeEach(() => {
  clearNuxtState()
  useState('session-loaded').value = true
})
const as = (role: string) => { useState('session').value = { username: 'kerry', role, csrf_token: 'ss_cs_x' } }
const mount = async () => { const w = await mountSuspended(Apps); await flush(8); return w }

describe('apps page', () => {
  it('lists my apps; admins can switch to everyone and see the User column', async () => {
    as('admin')
    const { calls } = stubFetch({
      'GET /api/apps': (c) => ({ body: c.url.includes('all=true') ? [grant(), grant({ id: 'ss_gr_2', username: 'op', client_name: 'Other' })] : [grant()] }),
    })
    const w = await mount()
    expect(w.text()).toContain('Claude Code')
    expect(w.text()).toContain('ss_cl_abc')
    expect(w.text()).toContain('Never')
    expect(w.findAll('th').map((h) => h.text())).not.toContain('User')
    await w.findAll('button').find((b) => b.text() === "Everyone's apps")!.trigger('click')
    await flush()
    expect(calls.at(-1)!.url).toBe('/api/apps?all=true')
    expect(w.findAll('th').map((h) => h.text())).toContain('User')
    expect(w.text()).toContain('Other')
    w.unmount()
  })

  it('non-admins get no segmented control', async () => {
    as('operator')
    stubFetch({ 'GET /api/apps': { body: [grant()] } })
    const w = await mount()
    expect(w.text()).not.toContain("Everyone's apps")
    w.unmount()
  })

  it('revoke confirms and DELETEs /api/apps/ss_gr_1, then the row disappears', async () => {
    as('operator')
    const { calls } = stubFetch({ 'GET /api/apps': { body: [grant()] }, 'DELETE /api/apps/ss_gr_1': { status: 204 } })
    const w = await mount()
    await w.findAll('tbody button').find((b) => b.text() === 'Revoke')!.trigger('click')
    expect(w.text()).toContain('Disconnect Claude Code?')
    expect(w.text()).toContain('It loses access right away and will have to ask again.')
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false)
    await w.findAll('dialog button').find((b) => b.text() === 'Disconnect')!.trigger('click')
    await flush()
    expect(calls.find((c) => c.method === 'DELETE')!.headers.get('x-csrf-token')).toBe('ss_cs_x')
    expect(w.find('tbody').exists() ? w.find('tbody').text() : '').not.toContain('Claude Code')
    w.unmount()
  })

  it('empty state text', async () => {
    as('viewer')
    stubFetch({ 'GET /api/apps': { body: [] } })
    const w = await mount()
    expect(w.text()).toContain('No connected apps. When the starstar CLI or an MCP client like Claude Code signs in, it shows up here.')
    w.unmount()
  })
})
