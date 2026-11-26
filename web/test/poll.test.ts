import { describe, expect, it } from 'vitest'
import { MAX_POLL_MS, POLL_MS, nextDelay, startPoller, type PollerEnv } from '~/utils/poll'

function fakeEnv() {
  const timers: { fn: () => void; ms: number; id: number }[] = []
  const scheduled: number[] = []
  let hidden = false
  let vis: (() => void) | null = null
  let next = 1
  const env: PollerEnv = {
    setTimeout(fn, ms) { const id = next++; timers.push({ fn, ms, id }); scheduled.push(ms); return id },
    clearTimeout(h) { const i = timers.findIndex((t) => t.id === h); if (i >= 0) timers.splice(i, 1) },
    isHidden: () => hidden,
    onVisibility(fn) { vis = fn; return () => { vis = null } },
  }
  return {
    env, timers, scheduled,
    fire() { const t = timers.shift(); t?.fn() },
    setHidden(h: boolean) { hidden = h; vis?.() },
    subscribed: () => vis !== null,
  }
}
const settle = async () => { for (let i = 0; i < 5; i++) await Promise.resolve() }

describe('nextDelay', () => {
  it('follows 5 → 10 → 20 → 30 → 30 and resets', () => {
    let d = POLL_MS
    const seq = [false, false, false, false].map(() => (d = nextDelay(d, false)))
    expect(seq).toEqual([10_000, 20_000, 30_000, 30_000])
    expect(MAX_POLL_MS).toBe(30_000)
    expect(nextDelay(30_000, true)).toBe(5_000)
  })
})

describe('startPoller', () => {
  it('backs_off_and_resets', async () => {
    const f = fakeEnv()
    const results = [false, false, true]
    let i = 0
    startPoller(async () => results[i++] ?? true, f.env)
    await settle()
    f.fire(); await settle()
    f.fire(); await settle()
    expect(f.scheduled).toEqual([10_000, 20_000, 5_000])
  })

  it('never_overlaps', async () => {
    const f = fakeEnv()
    let calls = 0
    const p = startPoller(() => { calls++; return new Promise<boolean>(() => {}) }, f.env)
    await settle()
    expect(f.timers).toHaveLength(0)
    expect(calls).toBe(1)
    const a = p.kick()
    const b = p.kick()
    expect(a).toBe(b)
    expect(calls).toBe(1)
  })

  it('kick ticks now and cancels the pending timer', async () => {
    const f = fakeEnv()
    let calls = 0
    const p = startPoller(async () => { calls++; return true }, f.env)
    await settle()
    expect(f.timers).toHaveLength(1)
    await p.kick()
    expect(calls).toBe(2)
    expect(f.timers).toHaveLength(1)
  })

  it('pauses while hidden and ticks on becoming visible', async () => {
    const f = fakeEnv()
    let calls = 0
    startPoller(async () => { calls++; return true }, f.env)
    await settle()
    f.setHidden(true)
    expect(f.timers).toHaveLength(0)
    f.setHidden(false)
    await settle()
    expect(calls).toBe(2)
    expect(f.timers).toHaveLength(1)
  })

  it('does not schedule when a tick settles while hidden', async () => {
    const f = fakeEnv()
    let resolve!: (v: boolean) => void
    startPoller(() => new Promise<boolean>((r) => { resolve = r }), f.env)
    f.setHidden(true)
    resolve(true)
    await settle()
    expect(f.timers).toHaveLength(0)
  })

  it('stop cancels the pending timer and unsubscribes', async () => {
    const f = fakeEnv()
    const p = startPoller(async () => true, f.env)
    await settle()
    expect(f.timers).toHaveLength(1)
    p.stop()
    expect(f.timers).toHaveLength(0)
    expect(f.subscribed()).toBe(false)
  })

  it('treats a throwing tick as a failure', async () => {
    const f = fakeEnv()
    startPoller(async () => { throw new Error('x') }, f.env)
    await settle()
    expect(f.scheduled).toEqual([10_000])
  })
})
