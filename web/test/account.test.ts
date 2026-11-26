import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Account from '~/pages/account.vue'

const { navigateToMock } = vi.hoisted(() => ({ navigateToMock: vi.fn() }))
mockNuxtImport('navigateTo', () => navigateToMock)

beforeEach(() => {
  clearNuxtState()
  navigateToMock.mockReset()
  useState('session').value = { username: 'kerry', role: 'admin', csrf_token: 'ss_cs_x' }
  useState('session-loaded').value = true
})

type W = Awaited<ReturnType<typeof mountSuspended>>
async function submit(w: W, cur: string, next: string, confirm = next) {
  await w.find('#current-password').setValue(cur)
  await w.find('#new-password').setValue(next)
  await w.find('#confirm-password').setValue(confirm)
  await w.find('form').trigger('submit')
  await flush()
}

describe('account page', () => {
  it('shows who is signed in', async () => {
    stubFetch({})
    const w = await mountSuspended(Account)
    expect(w.text()).toContain('Signed in as kerry (admin)')
  })

  it('password change sends ChangePassword with csrf and shows the revocation notice', async () => {
    const { calls } = stubFetch({ 'PUT /api/me/password': { status: 204 } })
    const w = await mountSuspended(Account)
    await submit(w, 'old-password', 'new-password-1')
    expect(calls).toHaveLength(1)
    expect(JSON.parse(calls[0]!.body!)).toEqual({ current_password: 'old-password', new_password: 'new-password-1' })
    expect(calls[0]!.headers.get('x-csrf-token')).toBe('ss_cs_x')
    expect(w.text()).toContain('Password changed. Your other sessions and all connected apps were signed out.')
    for (const id of ['#current-password', '#new-password', '#confirm-password']) expect((w.find(id).element as HTMLInputElement).value).toBe('')
  })

  it('wrong current password shows the 403 message', async () => {
    stubFetch({ 'PUT /api/me/password': { status: 403, body: { error: 'x' } } })
    const w = await mountSuspended(Account)
    await submit(w, 'nope', 'new-password-1')
    expect(w.text()).toContain('Your current password is incorrect.')
  })

  it('mismatch sends nothing', async () => {
    const { calls } = stubFetch({})
    const w = await mountSuspended(Account)
    await submit(w, 'old-password', 'new-password-1', 'different-1234')
    expect(w.text()).toContain("The new passwords don't match.")
    expect(calls).toHaveLength(0)
  })

  it('sign out goes to /login', async () => {
    const { calls } = stubFetch({ 'DELETE /api/session': { status: 204 } })
    const w = await mountSuspended(Account)
    await w.findAll('button').find((b) => b.text() === 'Sign out')!.trigger('click')
    await flush()
    expect(calls[0]!.method).toBe('DELETE')
    expect(navigateToMock).toHaveBeenCalledWith('/login')
  })
})
