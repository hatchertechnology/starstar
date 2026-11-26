<script setup lang="ts">
import type { Job } from '~/types/api'
import { formatDuration, formatPercent } from '~/utils/format'

const props = defineProps<{ job: Job | null }>()

function middle(s: string, max = 34): string {
  if (s.length <= max) return s
  const keep = max - 1
  const tail = Math.ceil(keep / 2)
  return `${s.slice(0, keep - tail)}…${s.slice(s.length - tail)}`
}
const fullName = computed(() => props.job?.file?.display_name ?? props.job?.file?.name ?? '')
const left = computed(() => (props.job ? `${props.job.inaccurate_estimates ? 'about ' : ''}${formatDuration(props.job.time_remaining)} left` : ''))
</script>

<template>
  <div class="job">
    <p class="file" :title="fullName">{{ job ? (fullName ? middle(fullName) : 'Print job') : 'No job' }}</p>
    <div class="meter" :class="{ empty: !job }" :aria-hidden="job ? undefined : 'true'">
      <progress max="100" :value="job ? Math.floor(job.progress) : 0" :aria-label="job ? 'Print progress' : undefined" />
      <span class="num pct">{{ job ? formatPercent(job.progress) : '' }}</span>
    </div>
    <p class="left num small muted">{{ left }}&nbsp;</p>
  </div>
</template>

<style scoped>
.job p { margin: 0; }
.file { font-weight: 600; overflow-wrap: anywhere; min-height: 1.4em; }
.meter { display: flex; align-items: center; gap: var(--s3); margin: var(--s1) 0; }
.meter.empty { visibility: hidden; }
.pct { min-width: 3.5rem; text-align: right; font-weight: 600; }
progress { transition: none; }
</style>
