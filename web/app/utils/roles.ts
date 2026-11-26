import type { Role } from '~/types/api'

export const ROLES: Role[] = ['viewer', 'operator', 'admin']

export function roleAtLeast(have: Role | null | undefined, need: Role): boolean {
  if (!have) return false
  return ROLES.indexOf(have) >= ROLES.indexOf(need)
}

export function roleMin(a: Role, b: Role): Role {
  return ROLES.indexOf(a) <= ROLES.indexOf(b) ? a : b
}
