import type { Job, PrinterStatus } from '~/types/api'
import { ApiRequestError } from '~/utils/api'
import { printerPath } from '~/utils/paths'
import { POLL_MS, startPoller, type Poller } from '~/utils/poll'

const TICK_TIMEOUT_MS = 15_000

/** Polls one printer's status and job: every 5 s while visible, backing off to 30 s on failure. */
export function usePrinterPoll(name: string) {
  const api = useApi()
  const status = ref<PrinterStatus | null>(null)
  const job = ref<Job | null>(null)
  const error = ref<ApiRequestError | null>(null)
  const lastOkAt = ref<number | null>(null)
  const retryInMs = ref(POLL_MS)
  let poller: Poller | null = null

  async function tick(): Promise<boolean> {
    const ctl = new AbortController()
    const timeout = setTimeout(() => ctl.abort(), TICK_TIMEOUT_MS)
    try {
      const [s, j] = await Promise.all([
        api<PrinterStatus>(printerPath(name, 'status'), { signal: ctl.signal }),
        api<Job>(printerPath(name, 'job'), { signal: ctl.signal }),
      ])
      status.value = s
      job.value = j
      error.value = null
      lastOkAt.value = Math.floor(Date.now() / 1000)
      return true
    } catch (e) {
      error.value = e instanceof ApiRequestError ? e : new ApiRequestError(0, "Can't reach starstar.")
      return false
    } finally {
      clearTimeout(timeout)
    }
  }

  async function tickAndNote(): Promise<boolean> {
    const ok = await tick()
    // The poller doubles its delay after this resolves; show the delay that will be used.
    retryInMs.value = ok ? POLL_MS : Math.min(Math.max(poller?.delay ?? POLL_MS, POLL_MS) * 2, 30_000)
    return ok
  }

  onMounted(() => {
    poller = startPoller(tickAndNote, {
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
      isHidden: () => document.visibilityState === 'hidden',
      onVisibility: (fn) => {
        document.addEventListener('visibilitychange', fn)
        return () => document.removeEventListener('visibilitychange', fn)
      },
    })
  })
  onBeforeUnmount(() => poller?.stop())

  async function refresh() {
    await poller?.kick()
  }

  return { status, job, error, lastOkAt, retryInMs, refresh }
}
