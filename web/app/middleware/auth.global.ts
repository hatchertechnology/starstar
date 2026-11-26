import { safeNext } from '~/utils/next'

export default defineNuxtRouteMiddleware(async (to) => {
  const session = useSession()
  try {
    await session.load()
  } catch {
    // Can't reach the API: treat as signed out so the sign-in page can show the error.
  }
  const isPublic = to.meta.public === true
  if (!isPublic && !session.user.value) {
    return navigateTo({ path: '/login', query: { next: to.fullPath } })
  }
  if (isPublic && session.user.value) {
    return navigateTo(safeNext(to.query.next), { replace: true })
  }
  const min = to.meta.minRole
  if (min && !session.can(min)) {
    useFlash().show(`That page needs the ${min} role.`)
    return navigateTo('/')
  }
})
