import { vi } from 'vitest'

export interface Reply { status?: number; body?: unknown; raw?: BodyInit | null; headers?: Record<string, string> }
export interface Call { method: string; path: string; url: string; headers: Headers; body: string | null }
type Handler = Reply | ((call: Call) => Reply)

/** Stubs global fetch. Keys are "METHOD /path" (query ignored); unknown routes answer 404. */
export function stubFetch(routes: Record<string, Handler>) {
  const calls: Call[] = []
  const fn = vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
    const url = String(input)
    const path = url.split('?')[0]!
    const method = (init.method ?? 'GET').toUpperCase()
    const call: Call = { method, path, url, headers: new Headers(init.headers), body: typeof init.body === 'string' ? init.body : null }
    calls.push(call)
    const h = routes[`${method} ${path}`]
    if (!h) return new Response(JSON.stringify({ error: 'no route' }), { status: 404 })
    const r = typeof h === 'function' ? h(call) : h
    const status = r.status ?? 200
    if (status === 204) return new Response(null, { status })
    const body = r.raw !== undefined ? r.raw : r.body === undefined ? null : JSON.stringify(r.body)
    return new Response(body, { status, headers: { 'content-type': 'application/json', ...r.headers } })
  })
  vi.stubGlobal('fetch', fn)
  return { calls, fn }
}

export const flush = async (n = 5) => { for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0)) }
