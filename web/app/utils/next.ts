const BLOCKED = ['/login', '/setup', '/api', '/oauth', '/mcp', '/.well-known']

/** Validates the attacker-controlled `next` parameter: only same-origin app paths survive. */
export function safeNext(next: unknown): string {
  if (typeof next !== 'string' || !next.startsWith('/')) return '/'
  if (next[1] === '/' || next[1] === '\\') return '/'
  for (let i = 0; i < next.length; i++) {
    const c = next.charCodeAt(i)
    if (c < 0x21 || c === 0x7f || c === 0x5c) return '/'
  }
  const path = next.split(/[?#]/, 1)[0]!
  if (BLOCKED.some((b) => path === b || path.startsWith(b + '/'))) return '/'
  return next
}
