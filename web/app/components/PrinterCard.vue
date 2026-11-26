<script setup lang="ts">
import type { PrinterSummary } from '~/types/api'
import { formatRelative, printerState } from '~/utils/format'

const props = defineProps<{ printer: PrinterSummary }>()
const emit = defineEmits<{ state: [name: string, tone: 'active' | 'hold' | 'fault' | 'rest' | 'offline'] }>()

const { status, job, error, lastOkAt, retryInMs } = usePrinterPoll(props.printer.name)
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | undefined
onMounted(() => { clock = setInterval(() => { now.value = Date.now() }, 1000) })
onBeforeUnmount(() => clearInterval(clock))

const waiting = computed(() => !status.value && !error.value)
const shadow = computed(() => !!error.value)
const tone = computed(() => (shadow.value ? 'offline' : status.value ? printerState(status.value.state).tone : null))
watch(tone, (t) => { if (t) emit('state', props.printer.name, t) }, { immediate: true })

const retry = computed(() => Math.round(retryInMs.value / 1000))
const lastSeen = computed(() => (lastOkAt.value ? formatRelative(lastOkAt.value, now.value) : null))
const updated = computed(() => (lastOkAt.value ? formatRelative(lastOkAt.value, now.value) : ''))
</script>

<template>
  <article class="tool card" :class="{ 'is-shadow': shadow, 'is-loading': waiting }" :aria-busy="waiting">
    <NuxtLink :to="`/printers/${encodeURIComponent(printer.name)}`" class="cover" :aria-label="`Open ${printer.name}`" />
    <header class="head">
      <PrinterSilhouette :kind="printer.kind" :variant="shadow || waiting ? 'shadow' : 'filled'" />
      <div class="id">
        <h2 class="name">{{ printer.name }}</h2>
        <p class="kind muted small">{{ printer.kind.toUpperCase() }}</p>
      </div>
      <StateBadge v-if="status && !shadow" :state="status.state" />
    </header>

    <div v-if="waiting" class="skeleton muted small">Checking…</div>
    <template v-else>
      <div class="temps">
        <TempReadout label="Nozzle" :now="status?.nozzle_temp" :target="shadow ? null : status?.nozzle_target" />
        <TempReadout label="Bed" :now="status?.bed_temp" :target="shadow ? null : status?.bed_target" />
      </div>
      <JobBlock :job="shadow ? null : job" />
      <p v-if="shadow" class="foot offline">
        Not answering · retrying in {{ retry }} s<template v-if="lastSeen"> · last seen {{ lastSeen }}</template>
      </p>
      <p v-else class="foot muted small num">Updated {{ updated }}</p>
    </template>
  </article>
</template>

<style scoped>
.card { position: relative; height: 15rem; display: flex; flex-direction: column; gap: var(--s3); overflow: hidden; transition: background-color var(--t-state) var(--ease); }
.cover { position: absolute; inset: 0; z-index: 1; border-radius: var(--radius); }
.cover:focus-visible { outline-offset: -2px; }
.head { display: flex; align-items: center; gap: var(--s3); }
.id { flex: 1; min-width: 0; }
.name { margin: 0; font-size: var(--step-1); overflow-wrap: anywhere; line-height: 1.2; }
.kind { margin: 0; }
.temps { display: grid; grid-template-columns: 1fr 1fr; gap: var(--s3); }
.foot { margin: auto 0 0; }
.offline { font-weight: 600; }
.skeleton { margin: auto 0; }
.is-shadow { background: transparent; border-style: dashed; }
.is-shadow .temps, .is-shadow :deep(.job) { opacity: 0.55; }
.card:hover:not(.is-shadow) { background: color-mix(in oklch, var(--tool), var(--board) 40%); }
</style>
