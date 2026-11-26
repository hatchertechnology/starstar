import type { Api } from '~/utils/api'
import { createApi } from '~/utils/api'

/** The app-wide API client: same-origin, session cookie, CSRF header on unsafe calls. */
export function useApi(): Api {
  const nuxtApp = useNuxtApp() as unknown as { _starstarApi?: Api }
  if (nuxtApp._starstarApi) return nuxtApp._starstarApi
  const app = useNuxtApp()
  const session = useSession()
  const router = useRouter()
  nuxtApp._starstarApi = createApi({
    fetch: (...a) => globalThis.fetch(...a),
    csrf: () => session.user.value?.csrf_token ?? null,
    onUnauthorized() {
      const route = router.currentRoute.value
      session.clear()
      if (route.meta.public) return
      void app.runWithContext(() => navigateTo({ path: '/login', query: { next: route.fullPath } }))
    },
  })
  return nuxtApp._starstarApi
}
