import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Login from '~/pages/login.vue'

const { navigateToMock } = vi.hoisted(() => ({ navigateToMock: vi.fn() }))
mockNuxtImport('navigateTo', () => navigateToMock)

beforeEach(() => {
  clearNuxtState()
  navigateToMock.mockReset()
})

async function fill(w: Awaited<ReturnType<typeof mountSuspended>>, u: string, p: string) {
  await w.find('#username').setValue(u)
  await w.find('#password').setValue(p)
  await w.find('form').trigger('submit')
  await flush()
}

describe('login', () => {
  it('shows the generic error on 401, keeps the username and clears the password', async () => {
    const { calls } = stubFetch({ 'POST /api/session': { status: 401, body: { error: 'bad' } } })
    const w = await mountSuspended(Login)
    await fill(w, 'kerry', 'wrong')
    expect(w.text()).toContain('Username or password is incorrect.')
    expect((w.find('#username').element as HTMLInputElement).value).toBe('kerry')
    expect((w.find('#password').element as HTMLInputElement).value).toBe('')
    expect(calls).toHaveLength(1)
    expect(navigateToMock).not.toHaveBeenCalled()
  })

  it('shows the lockout copy on 429', async () => {
    stubFetch({ 'POST /api/session': { status: 429, body: { error: 'slow' } } })
    const w = await mountSuspended(Login)
    await fill(w, 'kerry', 'x')
    expect(w.text()).toContain('Too many attempts. Wait 15 minutes and try again.')
  })

  it.each([
    ['/consent?request=abc', '/consent?request=abc'],
    ['//evil', '/'],
  ])('redirects to safeNext(%s) on success', async (next, want) => {
    stubFetch({ 'POST /api/session': { body: { username: 'kerry', role: 'admin', csrf_token: 'ss_cs_x' } } })
    const w = await mountSuspended(Login, { route: `/login?next=${encodeURIComponent(next)}` })
    await fill(w, 'kerry', 'pw')
    expect(navigateToMock).toHaveBeenCalledWith(want, { replace: true })
  })

  it('has the fixed copy and links to setup', async () => {
    stubFetch({})
    const w = await mountSuspended(Login)
    expect(w.text()).toContain('Sign in to starstar')
    expect(w.text()).toContain('Setting up a new starstar? Use the setup code.')
    expect(w.find('a[href="/setup"]').exists()).toBe(true)
  })
})
