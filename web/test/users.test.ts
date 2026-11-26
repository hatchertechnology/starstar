import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Users from '~/pages/users.vue'

const users = [
  { username: 'kerry', role: 'admin', disabled: false, created_at: 1764115200 },
  { username: 'op', role: 'operator', disabled: false, created_at: 1764115200 },
]

beforeEach(() => {
  clearNuxtState()
  useState('session').value = { username: 'kerry', role: 'admin', csrf_token: 'ss_cs_x' }
  useState('session-loaded').value = true
})

type W = Awaited<ReturnType<typeof mountSuspended>>
const mount = async () => {
  const w = await mountSuspended(Users)
  await flush(8)
  return w
}
const row = (w: W, name: string) => w.findAll('tbody tr').find((r) => r.find('th').text().startsWith(name))!

describe('users page', () => {
  it('lists users and marks the current one', async () => {
    stubFetch({ 'GET /api/users': { body: users } })
    const w = await mount()
    expect(row(w, 'kerry').text()).toContain('(you)')
    expect(row(w, 'op').text()).toContain('Active')
    w.unmount()
  })

  it('changing role PATCHes {"role":"operator"} and shows the new role', async () => {
    stubFetch({
      'GET /api/users': { body: [...users, { username: 'viewer1', role: 'viewer', disabled: false, created_at: 1 }] },
      'PATCH /api/users/viewer1': { body: { username: 'viewer1', role: 'operator', disabled: false, created_at: 1 } },
    })
    const w = await mount()
    const sel = row(w, 'viewer1').find('select')
    await sel.setValue('operator')
    await flush()
    const all = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls
    const patch = all.find(([u, i]) => u === '/api/users/viewer1' && i.method === 'PATCH')!
    expect(JSON.parse(patch[1].body as string)).toEqual({ role: 'operator' })
    expect((row(w, 'viewer1').find('select').element as HTMLSelectElement).value).toBe('operator')
    w.unmount()
  })

  it('a 409 on demoting the last admin reverts the select and shows the message', async () => {
    stubFetch({
      'GET /api/users': { body: [users[0]] },
      'PATCH /api/users/kerry': { status: 409, body: { error: 'cannot remove the last admin' } },
    })
    const w = await mount()
    await row(w, 'kerry').find('select').setValue('viewer')
    await flush()
    expect((row(w, 'kerry').find('select').element as HTMLSelectElement).value).toBe('admin')
    expect(row(w, 'kerry').text()).toContain('cannot remove the last admin')
    w.unmount()
  })

  it('disable PATCHes {"disabled":true}', async () => {
    stubFetch({
      'GET /api/users': { body: users },
      'PATCH /api/users/op': (c) => ({ body: { ...users[1], ...JSON.parse(c.body!) } }),
    })
    const w = await mount()
    await row(w, 'op').findAll('button').find((b) => b.text() === 'Disable')!.trigger('click')
    await flush()
    const calls = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls
    expect(JSON.parse(calls.find(([, i]) => i.method === 'PATCH')![1].body as string)).toEqual({ disabled: true })
    expect(row(w, 'op').text()).toContain('Disabled')
    expect(row(w, 'op').findAll('button').some((b) => b.text() === 'Enable')).toBe(true)
    w.unmount()
  })

  it('remove confirms first, then DELETEs /api/users/op', async () => {
    const { calls } = stubFetch({ 'GET /api/users': { body: users }, 'DELETE /api/users/op': { status: 204 } })
    const w = await mount()
    await row(w, 'op').findAll('button').find((b) => b.text() === 'Remove')!.trigger('click')
    expect(w.text()).toContain('Remove op?')
    expect(w.text()).toContain('Their sign-ins and connected apps stop working immediately.')
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false)
    await w.findAll('dialog button').find((b) => b.text() === 'Remove')!.trigger('click')
    await flush()
    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/api/users/op')).toBe(true)
    expect(w.findAll('tbody tr').some((r) => r.find('th').text().startsWith('op'))).toBe(false)
    w.unmount()
  })

  it('add user posts NewUser, clears the form and never keeps the password', async () => {
    const { calls } = stubFetch({
      'GET /api/users': { body: users },
      'POST /api/users': { status: 201, body: { username: 'new1', role: 'viewer', disabled: false, created_at: 1 } },
    })
    const w = await mount()
    await w.find('#new-username').setValue('new1')
    await w.find('#new-password').setValue('temp-pass-123')
    await w.find('form').trigger('submit')
    await flush()
    const post = calls.find((c) => c.method === 'POST')!
    expect(JSON.parse(post.body!)).toEqual({ username: 'new1', password: 'temp-pass-123', role: 'viewer' })
    expect(post.headers.get('x-csrf-token')).toBe('ss_cs_x')
    expect((w.find('#new-password').element as HTMLInputElement).value).toBe('')
    expect(w.text()).toContain('Share the password with them privately; they can change it under Account.')
    expect(w.html()).not.toContain('temp-pass-123')
    w.unmount()
  })

  it('add user handles 409', async () => {
    stubFetch({ 'GET /api/users': { body: users }, 'POST /api/users': { status: 409, body: { error: 'dup' } } })
    const w = await mount()
    await w.find('#new-username').setValue('op')
    await w.find('#new-password').setValue('temp-pass-123')
    await w.find('form').trigger('submit')
    await flush()
    expect(w.text()).toContain('That username is taken.')
    w.unmount()
  })
})
