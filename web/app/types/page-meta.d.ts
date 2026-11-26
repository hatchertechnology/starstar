import type { Role } from '~/types/api'

declare module '#app' {
  interface PageMeta {
    /** Reachable without a session (sign-in, setup). */
    public?: boolean
    /** Minimum role needed to open the page. */
    minRole?: Role
  }
}

export {}
