<script setup lang="ts">
import { printerState } from '~/utils/format'

const props = defineProps<{ state: string }>()
const info = computed(() => printerState(props.state))
</script>

<template>
  <span class="badge" :class="`tone-${info.tone}`">
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" focusable="false">
      <path v-if="info.tone === 'active'" d="M4 2.5v11l9-5.5z" fill="currentColor" />
      <path v-else-if="info.tone === 'hold'" d="M4 2.5h3v11H4zM9 2.5h3v11H9z" fill="currentColor" />
      <path v-else-if="info.tone === 'fault'" d="M8 1.5l7 12.5H1z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
      <circle v-else cx="8" cy="8" r="5" fill="none" stroke="currentColor" stroke-width="1.8" />
    </svg>
    {{ info.label }}
  </span>
</template>

<style scoped>
.badge { display: inline-flex; align-items: center; gap: var(--s1); padding: 0 var(--s2); border-radius: 999px; font-size: var(--step--1); font-weight: 600; border: 2px solid var(--outline); background: var(--tool); color: var(--ink); white-space: nowrap; }
.tone-active { border-color: var(--st-active); color: var(--st-active-ink); }
.tone-hold { background: var(--st-hold); border-color: var(--st-hold); color: var(--st-hold-ink); }
.tone-fault { border-color: var(--st-fault); color: var(--st-fault-ink); }
</style>
