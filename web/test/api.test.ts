import { describe, expect, it, vi } from 'vitest'
import { ApiRequestError, createApi, errorMessage } from '~/utils/api'

function setup(res: () => Response | Promise<Response>, csrf: string | null = 'ss_cs_x') {
  const calls: { url: string; init: RequestInit }[] = []
  const fetch = vi.fn(async (url: string, init: RequestInit) => {
    calls.push({ url, init })
    return res()
  }) as unknown as typeof globalThis.fetch
  const onUnauthorized = vi.fn()
  const api = createApi({ fetch, csrf: () => csrf, onUnauthorized })
  return { api, calls, onUnauthorized }
}
const hdr = (c: { init: RequestInit }, n: string) => new Headers(c.init.headers).get(n)
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })

describe('createApi', () => {
  it('sends csrf on unsafe methods only', async () => {
    const { api, calls } = setup(() => new Response(null, { status: 204 }))
    await api('/api/x')
    for (const m of ['PUT', 'POST', 'PATCH', 'DELETE'] as const) await api('/api/x', { method: m })
    expect(hdr(calls[0]!, 'x-csrf-token')).toBeNull()
    for (const c of calls.slice(1)) expect(hdr(c, 'x-csrf-token')).toBe('ss_cs_x')
    expect(calls[0]!.init.credentials).toBe('same-origin')
    expect(hdr(calls[0]!, 'accept')).toBe('application/json')
  })
  it('returns null on 204 and parses json on 200', async () => {
    expect(await setup(() => new Response(null, { status: 204 })).api('/api/x')).toBeNull()
    expect(await setup(() => json({ a: 1 })).api('/api/x')).toEqual({ a: 1 })
  })
  it('throws ApiRequestError with server message', async () => {
    const { api } = setup(() => json({ error: 'printer is busy' }, 409))
    await expect(api('/api/x')).rejects.toMatchObject({ status: 409, message: 'printer is busy' })
    const e = await api('/api/x').catch((x) => x)
    expect(e).toBeInstanceOf(ApiRequestError)
    expect(errorMessage(e)).toBe('printer is busy')
  })
  it('reads error_description', async () => {
    const { api } = setup(() => json({ error: 'invalid_request', error_description: 'bad thing' }, 400))
    await expect(api('/api/x')).rejects.toMatchObject({ message: 'bad thing' })
  })
  it('falls back to status messages', async () => {
    const { api } = setup(() => new Response('', { status: 502 }))
    await expect(api('/api/x')).rejects.toMatchObject({ status: 502, message: "The printer isn't answering." })
    await expect(setup(() => new Response('<html>', { status: 504 })).api('/x')).rejects.toMatchObject({ message: 'The printer took too long to answer.' })
  })
  it('calls onUnauthorized on 401 unless skipped', async () => {
    const a = setup(() => json({ error: 'nope' }, 401))
    await expect(a.api('/api/x')).rejects.toBeInstanceOf(ApiRequestError)
    expect(a.onUnauthorized).toHaveBeenCalledTimes(1)
    await expect(a.api('/api/x', { skipUnauthorizedHook: true })).rejects.toBeInstanceOf(ApiRequestError)
    expect(a.onUnauthorized).toHaveBeenCalledTimes(1)
  })
  it('maps network failure to status 0', async () => {
    const api = createApi({ fetch: (async () => { throw new TypeError('x') }) as unknown as typeof fetch, csrf: () => null, onUnauthorized() {} })
    await expect(api('/api/x')).rejects.toMatchObject({ status: 0, message: "Can't reach starstar." })
  })
  it('maps an aborted request (client timeout) to its own message', async () => {
    const api = createApi({ fetch: (async () => { throw new DOMException('aborted', 'AbortError') }) as unknown as typeof fetch, csrf: () => null, onUnauthorized() {} })
    await expect(api('/api/x')).rejects.toMatchObject({ status: 0, message: 'The printer is taking too long to answer.' })
  })
  it('returns a Blob when as=blob', async () => {
    const { api } = setup(() => new Response(new Uint8Array([1, 2, 3]), { status: 200, headers: { 'content-type': 'image/png' } }))
    const b = await api<Blob>('/api/x', { as: 'blob' })
    expect(b!.size).toBe(3)
    expect(b!.type).toBe('image/png')
  })
  it('serialises query and json bodies', async () => {
    const { api, calls } = setup(() => new Response(null, { status: 204 }))
    await api('/api/apps', { query: { all: true, skip: undefined, q: 'a b' } })
    expect(calls[0]!.url).toBe('/api/apps?all=true&q=a+b')
    await api('/api/x', { method: 'POST', json: { a: 1 } })
    expect(hdr(calls[1]!, 'content-type')).toBe('application/json')
    expect(calls[1]!.init.body).toBe('{"a":1}')
  })
  it('errorMessage is generic for unknown errors', () => {
    expect(errorMessage(new Error('x'))).toBe('Something went wrong.')
  })
})
