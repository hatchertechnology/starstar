export const POLL_MS = 5_000
export const MAX_POLL_MS = 30_000

/** The next delay: a success resets to 5 s; a failure doubles up to 30 s. */
export function nextDelay(current: number, ok: boolean): number {
  return ok ? POLL_MS : Math.min(Math.max(current, POLL_MS) * 2, MAX_POLL_MS)
}

export interface PollerEnv {
  setTimeout(fn: () => void, ms: number): unknown
  clearTimeout(h: unknown): void
  isHidden(): boolean
  onVisibility(fn: () => void): () => void
}

export interface Poller {
  stop(): void
  kick(): Promise<void>
  readonly delay: number
}

/**
 * Runs `tick` now and then again after each one settles (never overlapping),
 * only while the page is visible. `tick` resolves true on success.
 */
export function startPoller(tick: () => Promise<boolean>, env: PollerEnv): Poller {
  let delay = POLL_MS
  let timer: unknown = null
  let inflight: Promise<void> | null = null
  let stopped = false

  const cancel = () => {
    if (timer !== null) env.clearTimeout(timer)
    timer = null
  }
  const schedule = () => {
    cancel()
    if (stopped || env.isHidden()) return
    timer = env.setTimeout(() => { timer = null; void run() }, delay)
  }
  const run = (): Promise<void> => {
    if (inflight) return inflight
    if (stopped) return Promise.resolve()
    cancel()
    inflight = tick()
      .catch(() => false)
      .then((ok) => {
        delay = nextDelay(delay, ok)
        inflight = null
        schedule()
      })
    return inflight
  }

  const unsubscribe = env.onVisibility(() => {
    if (stopped) return
    if (env.isHidden()) cancel()
    else if (!inflight) void run()
  })

  void run()

  return {
    stop() {
      stopped = true
      cancel()
      unsubscribe()
    },
    kick: run,
    get delay() { return delay },
  }
}
