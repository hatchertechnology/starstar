import { beforeEach, describe, expect, it } from 'vitest'
import { clearNuxtState } from '#app'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flush, stubFetch } from './helpers'
import ConsentForm from '~/components/ConsentForm.vue'
import Consent from '~/pages/consent.vue'
import type { ConsentRequest } from '~/types/api'

const req = (o: Partial<ConsentRequest> = {}): ConsentRequest => ({ id: 'pa_1', client_id: 'ss_cl_x', client_name: 'Claude Code', client_name_self_declared: true, redirect_host: '127.0.0.1:33418', client_registered_at: 1764115200, requested_role: 'admin', user_role: 'admin', default_role: 'operator', max_role: 'admin', ...o })

type W = Awaited<ReturnType<typeof mountSuspended>>
const mountForm = (o: Partial<ConsentRequest> = {}) => mountSuspended(ConsentForm, { props: { request: req(o), csrf: 'csrf' } })

function data(w: W, which: 'allow' | 'deny'): FormData {
  const form = w.find(`[data-testid="${which}-form"]`).element as HTMLFormElement
  const submitter = form.querySelector(`button[value="${which}"]`) as HTMLButtonElement
  return new FormData(form, submitter)
}

beforeEach(() => {
  clearNuxtState()
})

describe('ConsentForm', () => {
  it('preselects default_role and does not include allow_admin', async () => {
    const w = await mountForm()
    const d = data(w, 'allow')
    expect(d.getAll('role')).toEqual(['operator'])
    expect(d.get('allow_admin')).toBeNull()
    expect(d.get('csrf_token')).toBe('csrf')
    expect(d.get('request')).toBe('pa_1')
    expect(d.get('decision')).toBe('allow')
  })

  it('admin needs the separate checkbox and never posts two roles', async () => {
    const w = await mountForm()
    const radios = w.findAll('input[type=radio]').map((r) => (r.element as HTMLInputElement).value)
    expect(radios).toEqual(['viewer', 'operator'])
    const box = w.find('input[type=checkbox]')
    await box.setValue(true)
    let d = data(w, 'allow')
    expect(d.getAll('role')).toEqual(['admin'])
    expect(d.get('allow_admin')).toBe('true')
    expect(w.findAll('input[type=radio]').every((r) => (r.element as HTMLInputElement).disabled)).toBe(true)
    await box.setValue(false)
    d = data(w, 'allow')
    expect(d.getAll('role')).toEqual(['operator'])
    expect(d.get('allow_admin')).toBeNull()
  })

  it('a chosen viewer is sent as viewer', async () => {
    const w = await mountForm()
    await w.findAll('input[type=radio]')[0]!.setValue(true)
    expect(data(w, 'allow').getAll('role')).toEqual(['viewer'])
  })

  it('caps choices at max_role', async () => {
    const w = await mountForm({ max_role: 'viewer', default_role: 'viewer', user_role: 'viewer' })
    expect(w.findAll('input[type=radio]')).toHaveLength(1)
    expect(w.find('input[type=checkbox]').exists()).toBe(false)
    expect(w.text()).toContain('Your account is a viewer, so this app can only view.')
    const op = await mountForm({ max_role: 'operator' })
    expect(op.find('input[type=checkbox]').exists()).toBe(false)
  })

  it('posts natively to /oauth/authorize/decision with allow and deny buttons', async () => {
    const w = await mountForm()
    const forms = w.findAll('form')
    for (const f of forms) {
      expect(f.attributes('method')).toBe('post')
      expect(f.attributes('action')).toBe('/oauth/authorize/decision')
    }
    expect(w.find('button[name=decision][value=allow]').text()).toBe('Allow')
    expect(w.find('button[name=decision][value=deny]').text()).toBe('Deny')
  })

  it('deny posts no role and no allow_admin, even with admin ticked', async () => {
    const w = await mountForm()
    await w.find('input[type=checkbox]').setValue(true)
    const d = data(w, 'deny')
    expect(d.getAll('role')).toHaveLength(0)
    expect(d.get('allow_admin')).toBeNull()
    expect(d.get('decision')).toBe('deny')
    expect(d.get('csrf_token')).toBe('csrf')
  })
})

describe('consent page', () => {
  const session = { username: 'kerry', role: 'admin', csrf_token: 'ss_cs_x' }
  const mountPage = async (query: string, routes: Parameters<typeof stubFetch>[0]) => {
    stubFetch(routes)
    useState('session').value = session
    useState('session-loaded').value = true
    const w = await mountSuspended(Consent, { route: `/consent${query}` })
    await flush()
    return w
  }

  it('shows the request facts and the self-declared warning', async () => {
    const w = await mountPage('?request=pa_1', { 'GET /oauth/authorize/requests/pa_1': { body: req() } })
    expect(w.text()).toContain('Claude Code wants to use your printers')
    expect(w.text()).toContain('self-declared name')
    expect(w.text()).toContain('Anyone can register an app with any name. Only continue if you just started this sign-in yourself.')
    expect(w.text()).toContain('127.0.0.1:33418')
    expect(w.text()).toContain('Signed in as')
    expect(w.text()).toContain('kerry (admin)')
    expect(w.find('form').exists()).toBe(true)
  })

  it('shows the self-declared warning only for dynamic clients', async () => {
    const w = await mountPage('?request=pa_1', { 'GET /oauth/authorize/requests/pa_1': { body: req({ client_name_self_declared: false, client_name: 'starstar CLI' }) } })
    expect(w.text()).not.toContain('self-declared')
  })

  it('expired request shows the expired message and no form', async () => {
    const w = await mountPage('?request=pa_1', { 'GET /oauth/authorize/requests/pa_1': { status: 404, body: { error: 'gone' } } })
    expect(w.text()).toContain('This sign-in request has expired or was already used')
    expect(w.text()).toContain('Go back to the app or terminal that sent you here and start again.')
    expect(w.find('form').exists()).toBe(false)
  })

  it('missing request id shows the expired message without calling the API', async () => {
    const w = await mountPage('', {})
    expect(w.text()).toContain('This sign-in request has expired or was already used')
    expect(w.find('form').exists()).toBe(false)
  })

  it.each([
    ['expired', 'This request expired. Start the sign-in again from your app.'],
    ['csrf', 'Your session check failed. Reload this page and try again.'],
    ['role', "That access level isn't allowed for this request. Choose again."],
    ['weird', 'Something went wrong with that decision. Try again.'],
  ])('error=%s shows a friendly message', async (code, text) => {
    const w = await mountPage(`?request=pa_1&error=${code}`, { 'GET /oauth/authorize/requests/pa_1': { body: req() } })
    expect(w.text()).toContain(text)
    expect(w.find('form').exists()).toBe(true)
  })

  it('error=expired with a gone request shows the expired heading and the message', async () => {
    const w = await mountPage('?request=pa_1&error=expired', { 'GET /oauth/authorize/requests/pa_1': { status: 404, body: { error: 'gone' } } })
    expect(w.text()).toContain('This request expired. Start the sign-in again from your app.')
    expect(w.find('form').exists()).toBe(false)
  })
})
