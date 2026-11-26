import { describe, expect, it } from 'vitest'
import { formatBytes, formatDate, formatDuration, formatPercent, formatRelative, formatTemp, printerState } from '~/utils/format'

describe('format', () => {
  it('durations', () => {
    expect(formatDuration(4320)).toBe('1 h 12 m')
    expect(formatDuration(240)).toBe('4 m')
    expect(formatDuration(30)).toBe('under 1 m')
    expect(formatDuration(null)).toBe('—')
    expect(formatDuration(undefined)).toBe('—')
  })
  it('percent and temp', () => {
    expect(formatPercent(42.9)).toBe('42 %')
    expect(formatTemp(214.6, 215)).toEqual({ now: '215°', target: '215°' })
    expect(formatTemp(24, 0).target).toBeNull()
    expect(formatTemp(null, undefined)).toEqual({ now: '—', target: null })
  })
  it('bytes', () => {
    expect(formatBytes(null)).toBe('—')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
  })
  it('relative', () => {
    const now = 1_800_000_000_000
    const s = now / 1000
    expect(formatRelative(s, now)).toBe('just now')
    expect(formatRelative(s - 3, now)).toBe('3 s ago')
    expect(formatRelative(s - 120, now)).toBe('2 min ago')
    expect(formatRelative(s - 5 * 3600, now)).toBe('5 h ago')
    expect(formatRelative(s - 3 * 86400, now)).toBe(formatDate(s - 3 * 86400))
  })
  it('formatDate uses Intl medium date', () => {
    const t = 1_764_115_200
    expect(formatDate(t)).toBe(new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(t * 1000)))
  })
  it('printer state', () => {
    expect(printerState('ATTENTION')).toEqual({ label: 'Needs attention', tone: 'hold' })
    expect(printerState('PRINTING')).toEqual({ label: 'Printing', tone: 'active' })
    expect(printerState('ERROR').tone).toBe('fault')
    expect(printerState('WEIRD')).toEqual({ label: 'Weird', tone: 'rest' })
  })
})
