import { beforeEach, describe, expect, it, vi } from 'vitest'
import { clearNuxtState } from '#app'
import { mockNuxtImport, mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch, type Reply } from './helpers'
import FileDetails from '~/pages/printers/[name]/files/[storage]/[...path].vue'
import type { FileEntry, Role } from '~/types/api'

const { navigateToMock } = vi.hoisted(() => ({ navigateToMock: vi.fn() }))
mockNuxtImport('navigateTo', () => navigateToMock)

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47])
const cube = (o: Partial<FileEntry> = {}): FileEntry => ({
  name: 'CUBE12~1.BGC', display_name: 'cube-12mm.bgcode', kind: 'print_file', read_only: false, size: 2048, modified: 1764115200, has_thumbnail: true,
  meta: { estimated_print_time: 3000, filament_type: 'PLA', layer_height: 0.2, nozzle_diameter: 0.4, printer_model: 'MK4IS', nozzle_temp: 225, bed_temp: 60, filament_used_g: 1.11 },
  ...o,
})
const FILE = '/api/printers/mk4/files/usb/cube-12mm.bgcode'
const THUMB = '/api/printers/mk4/thumbnails/usb/cube-12mm.bgcode'
const routes = (o: Record<string, Reply> = {}): Record<string, Reply> => ({
  'GET /api/printers/mk4': { body: { name: 'mk4', kind: 'mk4', host: '10.0.0.5', username: 'maker' } },
  'GET /api/printers/mk4/info': { body: { name: 'mk4', kind: 'mk4', firmware: '6.2', api_version: '2', nozzle_diameter: 0.4 } },
  'GET /api/printers/mk4/status': { body: { name: 'mk4', state: 'IDLE' } },
  'GET /api/printers/mk4/job': { status: 204 },
  [`GET ${FILE}`]: { body: cube() },
  [`HEAD ${FILE}`]: { status: 200, headers: { 'currently-printed': 'false' } },
  [`GET ${THUMB}`]: { raw: PNG, headers: { 'content-type': 'image/png' } },
  ...o,
})

beforeEach(() => {
  clearNuxtState()
  navigateToMock.mockReset()
  vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn(() => 'blob:thumb'), revokeObjectURL: vi.fn() }))
})
const signIn = (role: Role) => {
  useState('session').value = { username: 'u', role, csrf_token: 'ss_cs_x' }
  useState('session-loaded').value = true
}
const mount = async (role: Role = 'admin', route = '/printers/mk4/files/usb/cube-12mm.bgcode') => {
  signIn(role)
  const w = await mountSuspended(FileDetails, { route, attachTo: document.body })
  await flush(10)
  return w
}
const btn = (w: Awaited<ReturnType<typeof mount>>, label: string) => w.findAll('button').filter((b) => b.text() === label)

describe('file details page', () => {
  it('shows the preview and the slicer details', async () => {
    stubFetch(routes())
    const w = await mount()
    expect(w.find('h1').text()).toBe('cube-12mm.bgcode')
    const img = w.find('img')
    expect(img.attributes('src')).toBe('blob:thumb')
    expect(img.attributes('alt')).toBe('Preview of cube-12mm.bgcode')
    const dl = w.find('dl').text()
    for (const t of ['MK4IS', 'PLA', '0.2 mm', '0.4 mm', '50 m', '1.11 g', '225', '60']) expect(dl).toContain(t)
    w.unmount()
  })

  it('shows "No preview in this file" without a thumbnail or when it fails', async () => {
    const { calls } = stubFetch(routes({ [`GET ${FILE}`]: { body: cube({ has_thumbnail: false }) } }))
    let w = await mount()
    expect(w.text()).toContain('No preview in this file')
    expect(calls.some((c) => c.path === THUMB)).toBe(false)
    w.unmount()
    stubFetch(routes({ [`GET ${THUMB}`]: { status: 404, body: { error: 'file has no preview' } } }))
    w = await mount()
    expect(w.text()).toContain('No preview in this file')
    expect(w.find('img').exists()).toBe(false)
    w.unmount()
  })

  it('falls back to "No preview in this file" when the image cannot be shown', async () => {
    stubFetch(routes())
    const w = await mount()
    await w.find('img').trigger('error')
    expect(w.find('img').exists()).toBe(false)
    expect(w.text()).toContain('No preview in this file')
    w.unmount()
  })

  it('shows buttons by role and disables Print while a job runs', async () => {
    stubFetch(routes())
    let w = await mount('viewer')
    expect(btn(w, 'Print')).toHaveLength(0)
    expect(btn(w, 'Delete')).toHaveLength(0)
    w.unmount()
    w = await mount('operator')
    expect(btn(w, 'Print')).toHaveLength(1)
    expect(btn(w, 'Delete')).toHaveLength(0)
    w.unmount()
    stubFetch(routes({ 'GET /api/printers/mk4/status': { body: { name: 'mk4', state: 'PRINTING' } } }))
    w = await mount('admin')
    expect(btn(w, 'Delete')).toHaveLength(1)
    expect(btn(w, 'Print')[0]!.attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('confirms, then starts the print', async () => {
    const { calls } = stubFetch(routes({ [`POST ${FILE}/print`]: { status: 204 } }))
    const w = await mount('operator')
    await btn(w, 'Print')[0]!.trigger('click')
    expect(w.text()).toContain('Start printing cube-12mm.bgcode on mk4?')
    expect(w.text()).toContain('Make sure the bed is clear.')
    await btn(w, 'Start print')[0]!.trigger('click')
    await flush()
    expect(calls.some((c) => c.method === 'POST' && c.path === `${FILE}/print`)).toBe(true)
    w.unmount()
  })

  it('deletes with the encoded path and returns to the folder', async () => {
    const enc = '/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode'
    const { calls } = stubFetch(routes({
      [`GET ${enc}`]: { body: cube({ name: 'café #1?%.bgcode', display_name: 'café #1?%.bgcode', has_thumbnail: false }) },
      [`HEAD ${enc}`]: { status: 200 },
      [`DELETE ${enc}`]: { status: 204 },
    }))
    const w = await mount('admin', '/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode')
    expect(w.find('h1').text()).toBe('café #1?%.bgcode')
    await btn(w, 'Delete')[0]!.trigger('click')
    expect(w.text()).toContain('Delete café #1?%.bgcode?')
    expect(w.text()).toContain("This removes it from usb on mk4. This can't be undone.")
    await btn(w, 'Delete').at(-1)!.trigger('click')
    await flush()
    expect(calls.some((c) => c.method === 'DELETE' && c.path === enc)).toBe(true)
    expect(navigateToMock).toHaveBeenCalledWith({ path: '/printers/mk4', query: { storage: 'usb', dir: 'My Parts' } })
    w.unmount()
  })

  it('says when the file is gone', async () => {
    stubFetch(routes({ [`GET ${FILE}`]: { status: 404, body: { error: 'not found' } } }))
    const w = await mount()
    expect(w.text()).toContain('This file is no longer on the printer.')
    expect(w.find('a[href="/printers/mk4?storage=usb"]').exists()).toBe(true)
    w.unmount()
  })

  it.each([
    ['Print', 'Start print', 'POST', `${FILE}/print`],
    ['Delete', 'Delete', 'DELETE', FILE],
  ])('switches to "gone" when %s finds the file removed', async (open, confirmLabel, method, path) => {
    stubFetch(routes({ [`${method} ${path}`]: { status: 404, body: { error: 'not found' } } }))
    const w = await mount('admin')
    await btn(w, open)[0]!.trigger('click')
    await btn(w, confirmLabel).at(-1)!.trigger('click')
    await flush()
    expect(w.text()).toContain('This file is no longer on the printer.')
    expect(w.find('dl').exists()).toBe(false)
    expect(navigateToMock).not.toHaveBeenCalled()
    w.unmount()
  })

  it('shows printing now from the HEAD headers', async () => {
    stubFetch(routes({ [`HEAD ${FILE}`]: { status: 200, headers: { 'currently-printed': 'true' } } }))
    const w = await mount()
    expect(w.find('dl').text()).toContain('Printing now')
    w.unmount()
  })

  it('shows an explicit false printing status', async () => {
    stubFetch(routes({ [`HEAD ${FILE}`]: { status: 200, headers: { 'currently-printed': 'false' } } }))
    const w = await mount()
    expect(w.find('dl').text()).toContain('Printing nowno')
    w.unmount()
  })

  it('warns when the file was sliced for a different nozzle', async () => {
    stubFetch(routes({ 'GET /api/printers/mk4/info': { body: { name: 'mk4', kind: 'mk4', firmware: '6.2', api_version: '2', nozzle_diameter: 0.6 } } }))
    const w = await mount()
    expect(w.text()).toContain('This file was sliced for MK4IS with a 0.4 mm nozzle. mk4 is a MK4 with a 0.6 mm nozzle.')
    w.unmount()
  })
})

describe('file details routing', () => {
  it('a details URL renders the details page on its own, not inside the printer page', () => {
    const r = useRouter().resolve('/printers/mk4/files/usb/My%20Parts/cube-12mm.bgcode')
    expect(r.matched).toHaveLength(1)
    expect(r.params).toMatchObject({ name: 'mk4', storage: 'usb', path: ['My Parts', 'cube-12mm.bgcode'] })
    expect(useRouter().resolve('/printers/mk4').matched).toHaveLength(1)
  })
})
