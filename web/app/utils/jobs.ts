import type { Job } from '~/types/api'

export type JobAction = 'pause' | 'resume' | 'continue' | 'stop'

/** Which job controls apply for a printer state. */
export function jobActions(printerState: string, job: Job | null): JobAction[] {
  if (!job) return []
  switch (printerState.toUpperCase()) {
    case 'PRINTING': return ['pause', 'stop']
    case 'PAUSED': return ['resume', 'stop']
    case 'ATTENTION': return ['continue', 'stop']
    default: return []
  }
}
