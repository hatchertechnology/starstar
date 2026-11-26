export function formatTemp(current?: number | null, target?: number | null): { now: string; target: string | null } {
  const t = (n: number) => `${Math.round(n)}°`
  return {
    now: current == null ? '—' : t(current),
    target: target ? t(target) : null,
  }
}

export function formatDuration(seconds?: number | null): string {
  if (seconds == null) return '—'
  if (seconds < 60) return 'under 1 m'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h > 0 ? `${h} h ${m} m` : `${m} m`
}

/** PrusaLink progress is already a percentage (0–100). */
export function formatPercent(p: number): string {
  return `${Math.floor(p)} %`
}

export function formatBytes(n?: number | null): string {
  if (n == null) return '—'
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  return `${v.toFixed(1)} ${units[i]}`
}

export function formatDate(epochSeconds: number): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(epochSeconds * 1000))
}

export function formatRelative(epochSeconds: number, nowMs: number): string {
  const s = Math.floor(nowMs / 1000 - epochSeconds)
  if (s < 3) return 'just now'
  if (s < 60) return `${s} s ago`
  if (s < 3600) return `${Math.floor(s / 60)} min ago`
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`
  return formatDate(epochSeconds)
}

export type StateTone = 'active' | 'hold' | 'fault' | 'rest'

const STATES: Record<string, { label: string; tone: StateTone }> = {
  PRINTING: { label: 'Printing', tone: 'active' },
  BUSY: { label: 'Busy', tone: 'active' },
  PAUSED: { label: 'Paused', tone: 'hold' },
  ATTENTION: { label: 'Needs attention', tone: 'hold' },
  ERROR: { label: 'Error', tone: 'fault' },
  IDLE: { label: 'Idle', tone: 'rest' },
  READY: { label: 'Ready', tone: 'rest' },
  FINISHED: { label: 'Finished', tone: 'rest' },
  STOPPED: { label: 'Stopped', tone: 'rest' },
}

export function printerState(state: string): { label: string; tone: StateTone } {
  const known = STATES[state.toUpperCase()]
  if (known) return known
  const lower = state.toLowerCase()
  return { label: lower.charAt(0).toUpperCase() + lower.slice(1), tone: 'rest' }
}
