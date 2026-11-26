import { describe, expect, it } from 'vitest'
import { ROLES, roleAtLeast, roleMin } from '~/utils/roles'

describe('roles', () => {
  it('orders roles', () => {
    expect(ROLES).toEqual(['viewer', 'operator', 'admin'])
    expect(roleAtLeast('operator', 'viewer')).toBe(true)
    expect(roleAtLeast('viewer', 'operator')).toBe(false)
    expect(roleAtLeast('admin', 'admin')).toBe(true)
    expect(roleAtLeast(null, 'viewer')).toBe(false)
    expect(roleAtLeast(undefined, 'viewer')).toBe(false)
    expect(roleMin('admin', 'operator')).toBe('operator')
    expect(roleMin('viewer', 'operator')).toBe('viewer')
  })
})
