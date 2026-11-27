import { beforeEach, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import Library from '~/pages/library/index.vue'
const { nav } = vi.hoisted(() => ({ nav: vi.fn() }))
mockNuxtImport('navigateTo', () => nav)
const part = { id: 7, name: 'cube', latest_model_revision: 2, setups: 3, slices: 4, preview_file_id: 8, updated_at: 1764115200 }
beforeEach(() => { clearNuxtState(); nav.mockReset() })
function signIn(role: string) { useState('session').value = { username: 'u', role, csrf_token: 'token' }; useState('session-loaded').value = true }
it('lists parts, searches and sorts for viewers', async () => {
  signIn('viewer'); const { calls } = stubFetch({ 'GET /api/parts': { body: [part, { ...part, id: 9, preview_file_id: null }] } })
  const w = await mountSuspended(Library, { route: '/library' }); await flush()
  expect(w.find('a[href="/library/7"]').text()).toBe('cube')
  expect(w.find('img').attributes('src')).toBe('/api/files/8/preview')
  const rows = w.findAll('tbody tr')
  expect(rows[0]!.findAll('td').map(cell => cell.text()).slice(1, 5)).toEqual(['cube', '2', '3', '4'])
  expect(rows[1]!.find('img').exists()).toBe(false)
  expect(rows[1]!.findAll('td')[0]!.text()).toBe('—')
  expect(w.text()).not.toContain('New part')
  await w.find('input[type=search]').setValue('cub'); await flush()
  expect(calls.some(c => c.url === '/api/parts?q=cub')).toBe(true)
  await w.find('select').setValue('name'); await flush()
  expect(calls.some(c => c.url === '/api/parts?q=cub&sort=name')).toBe(true); w.unmount()
})
it('creates a part for operators and navigates', async () => {
  signIn('operator'); const { calls } = stubFetch({ 'GET /api/parts': { body: [] }, 'POST /api/parts': { body: part } })
  const w = await mountSuspended(Library, { route: '/library' }); await flush()
  await w.find('button').trigger('click')
  await w.find('input[name=name]').setValue('cube')
  await w.find('form').trigger('submit'); await flush()
  expect(JSON.parse(calls.find(c => c.method === 'POST')!.body!)).toEqual({ name: 'cube' })
  expect(nav).toHaveBeenCalledWith('/library/7'); w.unmount()
})
it('keeps a creation error when a pending list request succeeds', async () => {
  signIn('operator')
  let resolveList!: (response: Response) => void
  const listResponse = new Promise<Response>(resolve => { resolveList = resolve })
  const { fn } = stubFetch({ 'POST /api/parts': { status: 400, body: { error: 'Creation failed' } } })
  const fetchDefault = fn.getMockImplementation()!
  fn.mockImplementation((input, init) => (init?.method ?? 'GET') === 'GET' ? listResponse : fetchDefault(input, init))
  const w = await mountSuspended(Library, { route: '/library' })
  await w.find('button').trigger('click'); await w.find('input[name=name]').setValue('cube')
  await w.find('form').trigger('submit'); await flush()
  expect(w.text()).toContain('Creation failed')
  resolveList(new Response(JSON.stringify([]))); await flush()
  expect(w.text()).toContain('Creation failed'); w.unmount()
})

it('disables creation controls until the request finishes', async () => {
  signIn('operator')
  let resolvePost!: (response: Response) => void
  const postResponse = new Promise<Response>(resolve => { resolvePost = resolve })
  const { fn } = stubFetch({ 'GET /api/parts': { body: [] } })
  const fetchDefault = fn.getMockImplementation()!
  fn.mockImplementation((input, init) => init?.method === 'POST' ? postResponse : fetchDefault(input, init))
  const w = await mountSuspended(Library, { route: '/library' }); await flush()
  await w.find('button').trigger('click'); await w.find('input[name=name]').setValue('cube')
  await w.find('form').trigger('submit'); await flush()
  expect(w.find('input[name=name]').attributes('disabled')).toBeDefined()
  expect(w.findAll('form button').every(button => button.attributes('disabled') !== undefined)).toBe(true)
  resolvePost(new Response(JSON.stringify(part))); await flush()
  expect(nav).toHaveBeenCalledWith('/library/7'); w.unmount()
})
