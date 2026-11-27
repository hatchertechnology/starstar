import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import FileBrowser from '~/components/FileBrowser.vue'
import type { FileEntry, Role } from '~/types/api'

const file = (name: string, o: Partial<FileEntry> = {}): FileEntry => ({ name, kind: 'print_file', read_only: false, size: 2048, modified: 1764115200, meta: { estimated_print_time: 4320, filament_type: 'PLA' }, ...o })
const folder = (name: string): FileEntry => ({ name, kind: 'folder', read_only: false })
const root: FileEntry = { name: 'usb', kind: 'folder', read_only: false, children: [file('zebra.bgcode'), folder('My Parts'), file('apple.bgcode'), folder('Archive')] }
const sub: FileEntry = { name: 'My Parts', kind: 'folder', read_only: false, children: [file('café #1?%.bgcode')] }

beforeEach(() => {
  clearNuxtState()
  useState('session').value = { username: 'u', role: 'admin', csrf_token: 'ss_cs_x' }
  useState('session-loaded').value = true
})

const routes = () => ({
  'GET /api/printers/mk4/storage': { body: [{ name: 'usb', kind: 'usb', path: '/usb', available: true }, { name: 'sdcard', kind: 'sd', path: '/sdcard', available: false }] },
  'GET /api/printers/mk4/files/usb': { body: root },
  'GET /api/printers/mk4/files/usb/My%20Parts': { body: sub },
})
const mount = async (role: Role = 'admin', state = 'IDLE') => {
  const w = await mountSuspended(FileBrowser, { props: { printer: 'mk4', role, printerState: state }, attachTo: document.body })
  await flush()
  return w
}
const rowNames = (w: Awaited<ReturnType<typeof mount>>) => w.findAll('tbody tr').map((r) => r.find('th, td').text())
const btn = (w: Awaited<ReturnType<typeof mount>>, label: string) => w.findAll('button').filter((b) => b.text() === label)

describe('FileBrowser', () => {
  it('folders sort first, then by name; only available storages become tabs', async () => {
    stubFetch(routes())
    const w = await mount()
    expect(rowNames(w)).toEqual(['Archive', 'My Parts', 'apple.bgcode', 'zebra.bgcode'])
    expect(w.findAll('[role=tab]').map((t) => t.text())).toEqual(['usb'])
    expect(w.text()).toContain('1 h 12 m')
    expect(w.text()).toContain('PLA')
    w.unmount()
  })

  it('lists root and enters folders with encoded paths', async () => {
    const { calls } = stubFetch(routes())
    const w = await mount()
    await w.findAll('button').find((b) => b.text() === 'My Parts')!.trigger('click')
    await flush()
    expect(calls.at(-1)!.path).toBe('/api/printers/mk4/files/usb/My%20Parts')
    expect(rowNames(w)).toEqual(['café #1?%.bgcode'])
    expect(w.find('nav[aria-label="Folder"]').text()).toContain('My Parts')
    w.unmount()
  })

  it('print and delete are role-gated and confirmed', async () => {
    const mk = async (role: Role) => { stubFetch(routes()); return mount(role) }
    let w = await mk('viewer')
    expect(btn(w, 'Print')).toHaveLength(0)
    expect(btn(w, 'Delete')).toHaveLength(0)
    w.unmount()
    w = await mk('operator')
    expect(btn(w, 'Print').length).toBe(2)
    expect(btn(w, 'Delete')).toHaveLength(0)
    w.unmount()

    const { calls } = stubFetch({ ...routes(), 'POST /api/printers/mk4/files/usb/apple.bgcode/print': { status: 204 }, 'DELETE /api/printers/mk4/files/usb/zebra.bgcode': { status: 204 } })
    w = await mount('admin')
    expect(btn(w, 'Print').length).toBe(2)
    expect(btn(w, 'Delete').length).toBe(2)
    await btn(w, 'Print')[0]!.trigger('click')
    expect(w.text()).toContain('Start printing apple.bgcode on mk4?')
    expect(w.text()).toContain('Make sure the bed is clear.')
    expect(calls.filter((c) => c.method !== 'GET')).toHaveLength(0)
    await btn(w, 'Start print')[0]!.trigger('click')
    await flush()
    expect(calls.at(-1)!.method + ' ' + calls.at(-1)!.path).toBe('POST /api/printers/mk4/files/usb/apple.bgcode/print')
    await btn(w, 'Delete')[1]!.trigger('click')
    expect(w.text()).toContain('Delete zebra.bgcode?')
    expect(w.text()).toContain("This removes it from usb on mk4. This can't be undone.")
    await btn(w, 'Delete').at(-1)!.trigger('click')
    await flush()
    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/api/printers/mk4/files/usb/zebra.bgcode')).toBe(true)
    w.unmount()
  })

  it('disables Print while a job is running', async () => {
    stubFetch(routes())
    const w = await mount('operator', 'PRINTING')
    expect(btn(w, 'Print').every((b) => (b.element as HTMLButtonElement).disabled)).toBe(true)
    w.unmount()
  })

  it('shows the empty-folder hint and an inline error', async () => {
    stubFetch({ ...routes(), 'GET /api/printers/mk4/files/usb': { body: { ...root, children: [] } } })
    let w = await mount()
    expect(w.text()).toContain('This folder is empty. Upload files with `starstar files upload`.')
    w.unmount()
    stubFetch({ ...routes(), 'GET /api/printers/mk4/files/usb': { status: 502, body: { error: 'printer unreachable' } } })
    w = await mount()
    expect(w.text()).toContain('printer unreachable')
    w.unmount()
  })
  it('print files link to their details page; folders stay buttons', async () => {
    stubFetch(routes())
    const w = await mount()
    const link = w.find('tbody a[href="/printers/mk4/files/usb/zebra.bgcode"]')
    expect(link.exists()).toBe(true)
    expect(link.text()).toBe('zebra.bgcode')
    expect(w.findAll('tbody a').map((a) => a.text())).not.toContain('My Parts')
    await w.findAll('button').find((b) => b.text() === 'My Parts')!.trigger('click')
    await flush()
    expect(w.find('tbody a').attributes('href')).toBe('/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode')
    w.unmount()
  })

  it('opens the folder named in the query', async () => {
    stubFetch(routes())
    const w = await mountSuspended(FileBrowser, { props: { printer: 'mk4', role: 'admin', printerState: 'IDLE' }, route: '/printers/mk4?storage=usb&dir=My%20Parts', attachTo: document.body })
    await flush()
    expect(rowNames(w)).toEqual(['café #1?%.bgcode'])
    w.unmount()
  })
})

