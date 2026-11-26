import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Setup from '~/pages/setup.vue'

const { navigateToMock } = vi.hoisted(() => ({ navigateToMock: vi.fn() }))
mockNuxtImport('navigateTo', () => navigateToMock)

beforeEach(() => {
  clearNuxtState()
  navigateToMock.mockReset()
})

async function fill(w: Awaited<ReturnType<typeof mountSuspended>>, code: string, pw: string, confirm = pw) {
  await w.find('#setup-code').setValue(code)
  await w.find('#username').setValue('kerry')
  await w.find('#password').setValue(pw)
  await w.find('#confirm').setValue(confirm)
  await w.find('form').trigger('submit')
  await flush()
}

describe('setup', () => {
  it('posts the code as typed and lands on / on 201', async () => {
    const { calls } = stubFetch({ 'POST /api/setup': { status: 201, body: { username: 'kerry', role: 'admin', csrf_token: 'ss_cs_x' } } })
    const w = await mountSuspended(Setup)
    await fill(w, ' Code-123 ', 'long-enough-pw')
    expect(JSON.parse(calls[0]!.body!)).toEqual({ setup_code: ' Code-123 ', username: 'kerry', password: 'long-enough-pw' })
    expect(navigateToMock).toHaveBeenCalledWith('/', { replace: true })
  })

  it("shows \"That setup code isn't right.\" on 403", async () => {
    stubFetch({ 'POST /api/setup': { status: 403, body: { error: 'invalid setup code' } } })
    const w = await mountSuspended(Setup)
    await fill(w, 'nope', 'long-enough-pw')
    expect(w.text()).toContain("That setup code isn't right.")
  })

  it('409 redirects to /login with the flash', async () => {
    stubFetch({ 'POST /api/setup': { status: 409, body: { error: 'done' } } })
    const w = await mountSuspended(Setup)
    await fill(w, 'x', 'long-enough-pw')
    expect(navigateToMock).toHaveBeenCalledWith('/login')
    expect(useState('flash').value).toBe('starstar is already set up. Sign in.')
  })

  it('shows the server message on 400', async () => {
    stubFetch({ 'POST /api/setup': { status: 400, body: { error: 'Lowercase letters, digits, dot, dash and underscore.' } } })
    const w = await mountSuspended(Setup)
    await fill(w, 'x', 'long-enough-pw')
    expect(w.text()).toContain('Lowercase letters, digits')
  })

  it('blocks mismatched passwords and short passwords without a request', async () => {
    const { calls } = stubFetch({})
    const w = await mountSuspended(Setup)
    await fill(w, 'x', 'long-enough-pw', 'different-pw-1')
    expect(w.text()).toContain("The passwords don't match.")
    await fill(w, 'x', 'short', 'short')
    expect(w.text()).toContain('Use at least 8 characters.')
    expect(calls).toHaveLength(0)
  })
})
