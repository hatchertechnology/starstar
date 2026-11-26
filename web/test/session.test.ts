import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import authMiddleware from '~/middleware/auth.global'
import DefaultLayout from '~/layouts/default.vue'

const { navigateToMock } = vi.hoisted(() => ({ navigateToMock: vi.fn() }))
mockNuxtImport('navigateTo', () => navigateToMock)

const session = (role = 'admin') => ({ username: 'kerry', role, csrf_token: 'ss_cs_x' })

beforeEach(() => {
  clearNuxtState()
  navigateToMock.mockReset()
})

async function inNuxt<T>(fn: () => T | Promise<T>): Promise<T> {
  return useNuxtApp().runWithContext(fn) as Promise<T>
}

describe('session', () => {
  it('load() treats 401 as signed out and does not redirect', async () => {
    stubFetch({ 'GET /api/session': { status: 401, body: { error: 'x' } } })
    const s = await inNuxt(() => useSession())
    await inNuxt(() => s.load())
    expect(s.user.value).toBeNull()
    expect(s.loaded.value).toBe(true)
    expect(navigateToMock).not.toHaveBeenCalled()
  })

  it('signIn stores SessionInfo and later PUTs carry its csrf token', async () => {
    const { calls } = stubFetch({
      'POST /api/session': { body: session() },
      'PUT /api/x': { status: 204 },
    })
    const s = await inNuxt(() => useSession())
    await s.signIn('kerry', 'pw')
    expect(s.user.value?.username).toBe('kerry')
    const api = await inNuxt(() => useApi())
    await api('/api/x', { method: 'PUT' })
    expect(calls.at(-1)!.headers.get('x-csrf-token')).toBe('ss_cs_x')
    expect(JSON.stringify(calls[0]!.body)).toContain('kerry')
  })

  it('guard sends anonymous users to /login?next=<path>', async () => {
    stubFetch({ 'GET /api/session': { status: 401, body: { error: 'x' } } })
    const to = { path: '/printers/mk4', fullPath: '/printers/mk4?a=1', meta: {}, query: {} }
    await inNuxt(() => (authMiddleware as any)(to, to))
    expect(navigateToMock).toHaveBeenCalledWith({ path: '/login', query: { next: '/printers/mk4?a=1' } })
  })

  it('guard sends signed-in users away from public pages to safeNext', async () => {
    stubFetch({ 'GET /api/session': { body: session() } })
    const to = { path: '/login', fullPath: '/login', meta: { public: true }, query: { next: '//evil.example' } }
    await inNuxt(() => (authMiddleware as any)(to, to))
    expect(navigateToMock).toHaveBeenCalledWith('/', { replace: true })
  })

  it('guard keeps viewers off /users', async () => {
    stubFetch({ 'GET /api/session': { body: session('viewer') } })
    const to = { path: '/users', fullPath: '/users', meta: { minRole: 'admin' }, query: {} }
    await inNuxt(() => (authMiddleware as any)(to, to))
    expect(navigateToMock).toHaveBeenCalledWith('/')
    expect(useState('flash').value).toBe('That page needs the admin role.')
  })

  it('guard lets an admin into /users', async () => {
    stubFetch({ 'GET /api/session': { body: session() } })
    const to = { path: '/users', fullPath: '/users', meta: { minRole: 'admin' }, query: {} }
    const r = await inNuxt(() => (authMiddleware as any)(to, to))
    expect(r).toBeUndefined()
  })

  it('shell shows Users only for admins', async () => {
    stubFetch({})
    useState('session').value = session('admin')
    let w = await mountSuspended(DefaultLayout)
    expect(w.text()).toContain('Users')
    expect(w.text()).toContain('kerry')
    expect(w.text()).toContain('Sign out')
    useState('session').value = session('viewer')
    w = await mountSuspended(DefaultLayout)
    expect(w.text()).not.toContain('Users')
    await flush()
  })
})
