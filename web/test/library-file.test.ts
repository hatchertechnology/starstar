import { beforeEach, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import LibraryFile from '~/pages/library/[id].vue'
const { nav } = vi.hoisted(() => ({ nav: vi.fn() }))
mockNuxtImport('navigateTo', () => nav)
const file = { id: 7, name: 'cube.bgcode', version: 1, size: 10, kind: 'bgcode', sha256: 'abc', parsed: true, has_preview: true, uploaded_by: 'u', uploaded_at: 1, meta: { printer_model: 'MK4IS', filament_type: 'PLA' } }
beforeEach(() => { clearNuxtState(); nav.mockReset(); vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn(() => 'blob:preview'), revokeObjectURL: vi.fn() })) })
async function mount(role = 'admin') { useState('session').value = { username: 'u', role, csrf_token: 'token' }; useState('session-loaded').value = true; const w = await mountSuspended(LibraryFile, { route: '/library/7', attachTo: document.body }); await flush(); return w }
it('shows preview and facts and confirms admin deletion', async () => {
  const { calls } = stubFetch({ 'GET /api/library/7': { body: file }, 'GET /api/library/7/preview': { raw: 'png' }, 'DELETE /api/library/7': { status: 204 } })
  const w = await mount(); expect(w.find('img').attributes('src')).toBe('blob:preview'); expect(w.find('dl').text()).toContain('MK4IS'); expect(w.find('dl').text()).not.toContain('Printing now'); expect(w.find('a[href="/api/library/7/download"]').exists()).toBe(true)
  await w.findAll('button').find(b => b.text() === 'Delete')!.trigger('click'); expect(w.text()).toContain('Delete cube.bgcode v1?'); expect(w.text()).toContain('Copies already on printers are not affected.')
  await w.findAll('button').filter(b => b.text() === 'Delete').at(-1)!.trigger('click'); await flush(); expect(calls.some(c => c.method === 'DELETE')).toBe(true); expect(nav).toHaveBeenCalledWith('/library'); w.unmount()
})
it('shows unparsed and missing preview for viewers', async () => {
  stubFetch({ 'GET /api/library/7': { body: { ...file, parsed: false, has_preview: false } } }); const w = await mount('viewer'); expect(w.text()).toContain("This file's slicer details could not be read."); expect(w.text()).toContain('No preview in this file'); expect(w.findAll('button').some(b => b.text() === 'Delete' && b.isVisible())).toBe(false); w.unmount()
})
it('shows a link back when missing', async () => {
  stubFetch({ 'GET /api/library/7': { status: 404 } }); const w = await mount(); expect(w.text()).toContain('This file is not in the library.'); expect(w.find('a[href="/library"]').exists()).toBe(true); w.unmount()
})

it('does not create a preview URL after unmount', async () => {
  let resolvePreview!: (response: Response) => void
  const previewResponse = new Promise<Response>(resolve => { resolvePreview = resolve })
  const { fn } = stubFetch({ 'GET /api/library/7': { body: file } })
  const fetchDefault = fn.getMockImplementation()!
  fn.mockImplementation((input, init) => String(input).endsWith('/preview') ? previewResponse : fetchDefault(input, init))
  const w = await mount()
  expect(fn.mock.calls.some(c => String(c[0]).endsWith('/preview'))).toBe(true)
  w.unmount()
  resolvePreview(new Response('png'))
  await flush()
  expect(URL.createObjectURL).not.toHaveBeenCalled()
})
