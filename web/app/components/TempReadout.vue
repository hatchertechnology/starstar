<script setup lang="ts">
import { formatTemp } from '~/utils/format'

const props = defineProps<{ label: 'Nozzle' | 'Bed'; now?: number | null; target?: number | null }>()
const t = computed(() => formatTemp(props.now, props.target))
</script>

<template>
  <div class="temp">
    <span class="label muted small">{{ label }}</span>
    <span class="now num">{{ t.now }}</span>
    <span v-if="t.target" class="target num muted small">/ {{ t.target }}</span>
  </div>
</template>

<style scoped>
.temp { display: flex; flex-direction: column; min-width: 0; }
.now { font-size: var(--step-2); font-weight: 600; line-height: var(--lh-readout); }
.target { min-height: 1.2em; }
</style>
