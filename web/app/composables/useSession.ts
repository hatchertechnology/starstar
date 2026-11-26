import type { LoginRequest, Role, SessionInfo, SetupRequest } from '~/types/api'
import { ApiRequestError, createApi } from '~/utils/api'
import { roleAtLeast } from '~/utils/roles'

// The session calls use a bare client: they must not trigger the unauthorized hook.
function bareApi(csrf: () => string | null) {
  return createApi({ fetch: (...a) => globalThis.fetch(...a), csrf, onUnauthorized() {} })
}

export function useSession() {
  const user = useState<SessionInfo | null>('session', () => null)
  const loaded = useState<boolean>('session-loaded', () => false)
  const api = bareApi(() => user.value?.csrf_token ?? null)

  async function load(): Promise<void> {
    if (loaded.value) return
    try {
      user.value = await api<SessionInfo>('/api/session', { skipUnauthorizedHook: true })
      loaded.value = true
    } catch (e) {
      if (e instanceof ApiRequestError && e.status === 401) {
        user.value = null
        loaded.value = true
      } else {
        user.value = null
        throw e
      }
    }
  }

  async function signIn(username: string, password: string): Promise<void> {
    const body: LoginRequest = { username, password }
    user.value = await api<SessionInfo>('/api/session', { method: 'POST', json: body, skipUnauthorizedHook: true })
    loaded.value = true
  }

  async function setup(setup_code: string, username: string, password: string): Promise<void> {
    const body: SetupRequest = { setup_code, username, password }
    user.value = await api<SessionInfo>('/api/setup', { method: 'POST', json: body, skipUnauthorizedHook: true })
    loaded.value = true
  }

  async function signOut(): Promise<void> {
    try {
      await api('/api/session', { method: 'DELETE', skipUnauthorizedHook: true })
    } finally {
      clear()
    }
  }

  /** Re-reads the session (role, CSRF token) after something changed it server-side. */
  async function refresh(): Promise<void> {
    try {
      user.value = await api<SessionInfo>('/api/session', { skipUnauthorizedHook: true })
    } catch (e) {
      if (e instanceof ApiRequestError && e.status === 401) user.value = null
      else throw e
    }
  }

  function clear() {
    user.value = null
    loaded.value = true
  }

  function can(role: Role): boolean {
    return roleAtLeast(user.value?.role, role)
  }

  return { user, loaded, load, signIn, setup, signOut, can, clear, refresh }
}
