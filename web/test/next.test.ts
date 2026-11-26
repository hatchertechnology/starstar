import { describe, expect, it } from 'vitest'
import { safeNext } from '~/utils/next'

describe('safeNext', () => {
  it.each([
    ['/consent?request=abc', '/consent?request=abc'],
    ['/printers/mk4', '/printers/mk4'],
    ['//evil.example', '/'],
    ['/\\evil.example', '/'],
    ['https://evil.example', '/'],
    ['javascript:alert(1)', '/'],
    ['', '/'],
    [undefined, '/'],
    [['/a', '/b'], '/'],
    ['/\u0000x', '/'],
    ['/login?next=/x', '/'],
    ['/api/me', '/'],
    ['/\t/evil.example', '/'],
    ['/\n//evil.example', '/'],
    ['/ /evil', '/'],
    ['/setup', '/'],
    ['/oauth/authorize?x=1', '/'],
    ['/mcp', '/'],
    ['/.well-known/x', '/'],
    ['/apiary', '/apiary'],
  ])('safeNext(%j) → %s', (i, o) => expect(safeNext(i)).toBe(o))
})
