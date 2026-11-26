export class ApiRequestError extends Error {
  constructor(public status: number, message: string) {
    super(message)
    this.name = 'ApiRequestError'
  }
}

export type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'

export interface RequestOptions {
  method?: Method
  json?: unknown
  query?: Record<string, string | boolean | undefined>
  signal?: AbortSignal
  as?: 'json' | 'blob'
  skipUnauthorizedHook?: boolean
}

export interface ApiDeps {
  fetch: typeof globalThis.fetch
  csrf: () => string | null
  onUnauthorized: () => void
}

export type Api = <T>(path: string, opts?: RequestOptions) => Promise<T | null>

const statusMessages: Record<number, string> = {
  401: 'Please sign in again.',
  403: "Your role doesn't allow this.",
  404: 'Not found.',
  409: 'The printer is busy or the request conflicts with its state.',
  429: 'Too many attempts. Wait a few minutes and try again.',
  502: "The printer isn't answering.",
  504: 'The printer took too long to answer.',
}

async function failure(res: Response): Promise<string> {
  try {
    const body = JSON.parse(await res.text()) as { error?: unknown; error_description?: unknown }
    if (typeof body.error_description === 'string' && body.error_description) return body.error_description
    if (typeof body.error === 'string' && body.error) return body.error
  } catch { /* not JSON */ }
  return statusMessages[res.status] ?? `Request failed (${res.status}).`
}

export function createApi(deps: ApiDeps): Api {
  return async <T>(path: string, opts: RequestOptions = {}): Promise<T | null> => {
    const method = opts.method ?? 'GET'
    const headers = new Headers({ Accept: 'application/json' })
    let body: string | undefined
    if (opts.json !== undefined) {
      headers.set('Content-Type', 'application/json')
      body = JSON.stringify(opts.json)
    }
    if (method !== 'GET') {
      const token = deps.csrf()
      if (token) headers.set('X-CSRF-Token', token)
    }
    let url = path
    if (opts.query) {
      const q = new URLSearchParams()
      for (const [k, v] of Object.entries(opts.query)) if (v !== undefined) q.set(k, String(v))
      const s = q.toString()
      if (s) url += `?${s}`
    }
    let res: Response
    try {
      res = await deps.fetch(url, { method, headers, body, credentials: 'same-origin', signal: opts.signal })
    } catch {
      throw new ApiRequestError(0, "Can't reach starstar.")
    }
    if (!res.ok) {
      if (res.status === 401 && !opts.skipUnauthorizedHook) deps.onUnauthorized()
      throw new ApiRequestError(res.status, await failure(res))
    }
    if (res.status === 204) return null
    if (opts.as === 'blob') return (await res.blob()) as T
    const text = await res.text()
    return text ? (JSON.parse(text) as T) : null
  }
}

export function errorMessage(e: unknown): string {
  return e instanceof ApiRequestError ? e.message : 'Something went wrong.'
}
