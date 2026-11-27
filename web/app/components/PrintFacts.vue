<script setup lang="ts">
import type { PrintMeta } from '~/types/api'
import { formatBytes, formatDate, formatDuration } from '~/utils/format'
withDefaults(defineProps<{ meta?: PrintMeta | null; size?: number | null; modified?: number | null; printingNow?: boolean | null }>(), { printingNow: undefined })
</script>

<template>
          <dl class="facts">
            <template v-if="meta?.printer_model"><dt>Sliced for</dt><dd>{{ meta.printer_model }}</dd></template>
            <template v-if="meta?.nozzle_diameter != null"><dt>Nozzle</dt><dd>{{ meta.nozzle_diameter }} mm</dd></template>
            <template v-if="meta?.layer_height != null"><dt>Layer height</dt><dd>{{ meta.layer_height }} mm</dd></template>
            <template v-if="meta?.filament_type"><dt>Filament</dt><dd>{{ meta.filament_type }}<template v-if="meta.filament_used_g != null"> · {{ meta.filament_used_g }} g</template></dd></template>
            <template v-if="meta?.nozzle_temp != null || meta?.bed_temp != null"><dt>Temperatures</dt><dd>nozzle {{ meta?.nozzle_temp ?? '—' }} °C · bed {{ meta?.bed_temp ?? '—' }} °C</dd></template>
            <template v-if="meta?.estimated_print_time != null"><dt>Estimated time</dt><dd>{{ formatDuration(meta.estimated_print_time) }}</dd></template>
            <dt>Size</dt><dd>{{ formatBytes(size) }}</dd>
            <template v-if="modified"><dt>Modified</dt><dd>{{ formatDate(modified) }}</dd></template>
            <template v-if="printingNow != null"><dt>Printing now</dt><dd>{{ printingNow ? 'yes' : 'no' }}</dd></template>
          </dl>
</template>

<style scoped>
.facts { display: grid; grid-template-columns: max-content 1fr; gap: var(--s2) var(--s4); margin: 0 0 var(--s4); }
.facts dt { color: var(--muted, inherit); }
.facts dd { margin: 0; }
</style>
