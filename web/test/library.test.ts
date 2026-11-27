import { beforeEach, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Library from '~/pages/library/index.vue'
const { nav } = vi.hoisted(() => ({ nav: vi.fn() }))
mockNuxtImport('navigateTo', () => nav)
const file = { id: 7, name: 'cube.bgcode', version: 1, size: 10, kind: 'bgcode', sha256: 'abc', parsed: true, has_preview: true, uploaded_by: 'u', uploaded_at: 1764115200, meta: { printer_model: 'MK4IS', filament_type: 'PLA', estimated_print_time: 300 } }
beforeEach(() => { clearNuxtState(); nav.mockReset() })
function signIn(role: string) { useState('session').value = { username: 'u', role, csrf_token: 'token' }; useState('session-loaded').value = true }
it('lists files and searches', async () => {
  signIn('viewer'); const { calls } = stubFetch({ 'GET /api/library': { body: [file] } })
  const w = await mountSuspended(Library, { route: '/library' }); await flush()
  expect(w.text()).toContain('MK4IS'); expect(w.find('a[href="/library/7"]').text()).toBe('cube.bgcode')
  expect(w.find('button').exists()).toBe(false)
  await w.find('input[type=search]').setValue('cub'); await flush()
  expect(calls.some(c => c.url === '/api/library?q=cub')).toBe(true); w.unmount()
})
it('uploads a file for operators', async () => {
  signIn('operator'); const { fn } = stubFetch({ 'GET /api/library': { body: [] }, 'POST /api/library': { body: file } })
  const w = await mountSuspended(Library, { route: '/library' }); await flush()
  expect(w.find('button').text()).toBe('Upload')
  const input = w.find('input[type=file]'); const upload = new File(['data'], 'cube.bgcode', { type: 'application/octet-stream' })
  Object.defineProperty(input.element, 'files', { value: [upload], configurable: true }); await input.trigger('change'); await flush()
  const call = fn.mock.calls.find(c => String(c[0]).startsWith('/api/library?name='))!
  expect(call[0]).toBe('/api/library?name=cube.bgcode'); expect(call[1]?.body).toBe(upload); expect(nav).toHaveBeenCalledWith('/library/7'); w.unmount()
})

it('keeps an upload error when a pending list request succeeds', async () => {
  signIn('operator')
  let resolveList!: (response: Response) => void
  const listResponse = new Promise<Response>(resolve => { resolveList = resolve })
  const { fn } = stubFetch({ 'POST /api/library': { status: 400, body: { error: 'Upload failed' } } })
  const fetchDefault = fn.getMockImplementation()!
  fn.mockImplementation((input, init) => (init?.method ?? 'GET') === 'GET' ? listResponse : fetchDefault(input, init))
  const w = await mountSuspended(Library, { route: '/library' })
  const input = w.find('input[type=file]')
  Object.defineProperty(input.element, 'files', { value: [new File(['data'], 'cube.bgcode')], configurable: true })
  await input.trigger('change'); await flush()
  expect(w.text()).toContain('Upload failed')
  resolveList(new Response(JSON.stringify([])))
  await flush()
  expect(w.text()).toContain('Upload failed')
  w.unmount()
})
