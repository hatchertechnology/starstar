import { describe, expect, it } from 'vitest'
import { jobActions } from '~/utils/jobs'
import type { Job } from '~/types/api'

const job = (state: string): Job => ({ id: 42, state, progress: 10, time_printing: 5, serial_print: false })

describe('jobActions', () => {
  it('maps states to actions', () => {
    expect(jobActions('PRINTING', job('PRINTING'))).toEqual(['pause', 'stop'])
    expect(jobActions('PAUSED', job('PAUSED'))).toEqual(['resume', 'stop'])
    expect(jobActions('ATTENTION', job('PRINTING'))).toEqual(['continue', 'stop'])
    expect(jobActions('IDLE', null)).toEqual([])
    expect(jobActions('FINISHED', job('FINISHED'))).toEqual([])
    expect(jobActions('PRINTING', null)).toEqual([])
  })
})
