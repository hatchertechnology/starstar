import { describe, expect, it } from 'vitest'
import { slicedMismatch } from '~/utils/compat'

describe('slicedMismatch', () => {
  it('is null when family and nozzle match', () => {
    expect(slicedMismatch({ printer_model: 'MK4IS', nozzle_diameter: 0.4 }, 'mk4', 0.4, 'mk4-2')).toBeNull()
  })
  it('reports a nozzle difference', () => {
    const m = slicedMismatch({ printer_model: 'MK4S', nozzle_diameter: 0.4 }, 'mk4', 0.6, 'mk4-2')
    expect(m).toBe('This file was sliced for MK4S with a 0.4 mm nozzle. mk4-2 is a MK4 with a 0.6 mm nozzle.')
  })
  it('reports a family difference', () => {
    const m = slicedMismatch({ printer_model: 'MINIIS', nozzle_diameter: 0.4 }, 'mk4', 0.4, 'mk4-2')
    expect(m).toBe('This file was sliced for MINIIS with a 0.4 mm nozzle. mk4-2 is a MK4 with a 0.4 mm nozzle.')
  })
  it('leaves out unknown parts and is null with nothing to compare', () => {
    expect(slicedMismatch({ printer_model: 'MINI' }, 'mk4', null, 'p')).toBe('This file was sliced for MINI. p is a MK4.')
    expect(slicedMismatch(null, 'mk4', 0.4, 'p')).toBeNull()
    expect(slicedMismatch({ filament_type: 'PLA' }, 'mk4', 0.4, 'p')).toBeNull()
  })
})
