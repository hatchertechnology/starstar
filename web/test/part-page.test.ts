import { beforeEach, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import PartPage from '~/pages/library/[id].vue'
const file = { id: 11, part_id: 7, model_revision_id: 2, setup_revision_id: null, role: 'source', name: 'cube.stl', size: 10, sha256: 'a', meta: null, parsed: false, has_preview: false, target: null, superseded_by: null, note: null, uploaded_by: 'u', uploaded_at: 1 }
const slice = { ...file, id: 12, name: 'cube.gcode', role: 'slice', target: { family: 'mk4', nozzle: 0.4, profile: 'Fast' }, meta: { filament_type: 'PLA' } }
const part = { id: 7, name: 'Cube', notes: 'Notes', default_setup_id: null, preview_file_id: null, model_revisions: [{ id: 2, number: 2, files: [file] }, { id: 1, number: 1, files: [] }], setups: [{ id: 3, name: 'Default', default_profile: null, behind_model: 'on model rev 1; latest is rev 2', revisions: [{ id: 4, number: 1, model_revision_id: 1, model_revision_number: 1, project: null, slices: [slice] }] }], other_files: [] }
beforeEach(() => clearNuxtState())
async function mount(role = 'admin') { useState('session').value = { username: 'u', role, csrf_token: 'token' }; useState('session-loaded').value = true; const w = await mountSuspended(PartPage, { route: '/library/7', attachTo: document.body }); await flush(); return w }
const button = (w: Awaited<ReturnType<typeof mount>>, text: string) => w.findAll('button').find(b => b.text() === text)!
it('renders the tree and hides writes from viewers', async () => { stubFetch({ 'GET /api/parts/7': { body: part } }); const w = await mount('viewer'); expect(w.text()).toContain('cube.stl'); expect(w.text()).toContain('Fast'); expect(w.text()).toContain('on model rev 1; latest is rev 2'); expect(w.text()).not.toContain('Upload slice'); expect(w.text()).not.toContain('Delete setup'); w.unmount() })
it('creates a revision with selected carry files', async () => { const { calls } = stubFetch({ 'GET /api/parts/7': { body: part }, 'POST /api/parts/7/revisions': { body: {} } }); const w = await mount('operator'); await button(w, 'New revision').trigger('click'); expect(w.text()).toContain('Setups built from rev 2 will stop matching automatically until they get a new revision built from the new revision.'); await w.find('form.panel input[type=checkbox]').setValue(true); await button(w, 'Create revision').trigger('click'); await flush(); expect(JSON.parse(calls.find(c => c.method === 'POST')!.body!).carry).toEqual([11]); w.unmount() })
it('uploads and offers replacement after a conflict', async () => { let count = 0; const { calls } = stubFetch({ 'GET /api/parts/7': { body: part }, 'POST /api/parts/7/files': () => ++count === 1 ? { status: 409, body: { error: 'Already exists' } } : { body: {} } }); const w = await mount('operator'); await button(w, 'Upload slice').trigger('click'); const input = w.find('input[type=file]'); Object.defineProperty(input.element, 'files', { value: [new File(['x'], 'part.gcode')], configurable: true }); await input.trigger('change'); await w.find('form.panel').trigger('submit'); await flush(); expect(w.text()).toContain('Already exists'); await button(w, 'Replace').trigger('click'); await flush(); expect(calls.filter(c => c.method === 'POST')[0]!.url).toContain('role=slice&setup=3&name=part.gcode'); expect(calls.filter(c => c.method === 'POST')[1]!.url).toContain('replace=true'); w.unmount() })
it('confirms setup deletion with counts', async () => { stubFetch({ 'GET /api/parts/7': { body: part } }); const w = await mount(); await button(w, 'Delete setup').trigger('click'); expect(w.text()).toContain('1 revision'); expect(w.text()).toContain('1 file'); w.unmount() })
it('locks revision controls while creation is pending', async () => {
  let resolve!: (response: Response) => void
  const pending = new Promise<Response>(r => { resolve = r })
  const { fn } = stubFetch({ 'GET /api/parts/7': { body: part } })
  const original = fn.getMockImplementation()!
  fn.mockImplementation((input, init) => String(input).includes('/revisions') && init?.method === 'POST' ? pending : original(input, init))
  const w = await mount('operator'); await button(w, 'New revision').trigger('click'); await w.find('form.panel').trigger('submit'); await flush()
  expect(button(w, 'Upload slice').attributes('disabled')).toBeDefined(); await button(w, 'Upload slice').trigger('click'); expect(w.find('input[type=file]').exists()).toBe(false); expect(w.find('form.panel fieldset').attributes('disabled')).toBeDefined(); expect(w.find('form.panel button[type=button]').attributes('disabled')).toBeDefined()
  resolve(new Response('{}')); await flush(); w.unmount()
})

it('starts each revision with a fresh carry draft', async () => {
  const { calls } = stubFetch({ 'GET /api/parts/7': { body: part }, 'POST /api/parts/7/revisions': { body: {} } })
  const w = await mount('operator')
  await button(w, 'New revision').trigger('click')
  await w.find('form.panel input[type=checkbox]').setValue(true)
  await w.find('form.panel').trigger('submit'); await flush()
  await button(w, 'New revision').trigger('click')
  expect((w.find('form.panel input[type=checkbox]').element as HTMLInputElement).checked).toBe(false)
  await w.find('form.panel').trigger('submit'); await flush()
  expect(calls.filter(c => c.method === 'POST').map(c => JSON.parse(c.body!).carry)).toEqual([[11], []])
  w.unmount()
})
it('clears replacement when the upload changes', async () => {
  stubFetch({ 'GET /api/parts/7': { body: part }, 'POST /api/parts/7/files': { status: 409, body: { error: 'Already exists' } } })
  const w = await mount('operator'); await button(w, 'Upload slice').trigger('click')
  const input = w.find('input[type=file]'); Object.defineProperty(input.element, 'files', { value: [new File(['x'], 'a.gcode')], configurable: true }); await input.trigger('change')
  await w.find('form.panel').trigger('submit'); await flush(); expect(button(w, 'Replace')).toBeTruthy()
  await w.find('form.panel input:not([type=file])').setValue('changed')
  expect(w.findAll('button').some(b => b.text() === 'Replace')).toBe(false)
  w.unmount()
})
it('renders gallery previews and slice facts for an operator', async () => {
  const render = { ...file, id: 13, role: 'render', name: 'render.png' }
  stubFetch({ 'GET /api/parts/7': { body: { ...part, model_revisions: [{ ...part.model_revisions[0], files: [render] }] } } })
  const w = await mount('operator')
  expect(w.find('img[alt="render.png"]').attributes('src')).toBe('/api/files/13/preview')
  expect(w.find('a[href="/api/files/13/download"]').exists()).toBe(true)
  expect(w.find('dl').text()).toContain('PLA')
  expect(w.find('input').exists()).toBe(true)
  expect(w.text()).toContain('Save profile')
  w.unmount()
})
it('clears replacement when the selected file changes', async () => {
  stubFetch({ 'GET /api/parts/7': { body: part }, 'POST /api/parts/7/files': { status: 409, body: { error: 'Already exists' } } })
  const w = await mount('operator'); await button(w, 'Upload slice').trigger('click')
  const input = w.find('input[type=file]')
  Object.defineProperty(input.element, 'files', { value: [new File(['x'], 'a.gcode')], configurable: true }); await input.trigger('change')
  await w.find('form.panel').trigger('submit'); await flush(); expect(button(w, 'Replace')).toBeTruthy()
  Object.defineProperty(input.element, 'files', { value: [new File(['y'], 'b.gcode')], configurable: true }); await input.trigger('change')
  expect(w.findAll('button').some(b => b.text() === 'Replace')).toBe(false)
  w.unmount()
})
it('lets an operator save the default profile', async () => {
  const { calls } = stubFetch({ 'GET /api/parts/7': { body: part }, 'PATCH /api/setups/3': { body: {} } })
  const w = await mount('operator')
  await w.find('article input:not([type=checkbox])').setValue('Fast')
  await w.find('article form').trigger('submit'); await flush()
  expect(JSON.parse(calls.find(c => c.method === 'PATCH')!.body!)).toEqual({ default_profile: 'Fast' })
  w.unmount()
})
