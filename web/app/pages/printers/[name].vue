<script setup lang="ts">
import type { PrinterSummary } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'
import { formatRelative } from '~/utils/format'
import { printerPath } from '~/utils/paths'

const route = useRoute()
const api = useApi()
const session = useSession()
const name = computed(() => String(route.params.name))
useHead({ title: computed(() => `${name.value} · starstar`) })

const printer = ref<PrinterSummary | null>(null)
const missing = ref(false)
const loadError = ref<string | null>(null)
const { status, job, error, lastOkAt, retryInMs, refresh } = usePrinterPoll(name.value)
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | undefined

onMounted(async () => {
  clock = setInterval(() => { now.value = Date.now() }, 1000)
  try {
    printer.value = await api<PrinterSummary>(printerPath(name.value))
  } catch (e) {
    if (e instanceof ApiRequestError && e.status === 404) missing.value = true
    else loadError.value = errorMessage(e)
  }
})
onBeforeUnmount(() => clearInterval(clock))

const shadow = computed(() => !!error.value)
const role = computed(() => session.user.value?.role ?? 'viewer')
const state = computed(() => status.value?.state ?? 'UNKNOWN')
</script>

<template>
  <section v-if="missing" class="tool">
    <h1>No printer called {{ name }}.</h1>
    <p><NuxtLink to="/">Back to all printers</NuxtLink></p>
  </section>

  <section v-else class="detail">
    <p class="back small"><NuxtLink to="/">← All printers</NuxtLink></p>
    <header class="head">
      <PrinterSilhouette :kind="printer?.kind ?? 'unknown'" :variant="shadow ? 'shadow' : 'filled'" />
      <div class="id">
        <h1>{{ name }}</h1>
        <p class="muted small">
          <template v-if="printer">{{ printer.kind.toUpperCase() }} · {{ printer.host }}</template>
        </p>
      </div>
      <StateBadge v-if="status && !shadow" :state="status.state" />
    </header>

    <Notice v-if="loadError" kind="error">{{ loadError }}</Notice>
    <Notice v-if="shadow" kind="error">
      Not answering · retrying in {{ Math.round(retryInMs / 1000) }} s<template v-if="lastOkAt"> · last seen {{ formatRelative(lastOkAt, now) }}</template>
    </Notice>

    <div class="cols">
      <CameraSnapshot :printer="name" />
      <div class="stack right">
        <div class="tool">
          <div class="temps">
            <TempReadout label="Nozzle" :now="status?.nozzle_temp" :target="status?.nozzle_target" />
            <TempReadout label="Bed" :now="status?.bed_temp" :target="status?.bed_target" />
          </div>
        </div>
        <div class="tool">
          <JobBlock :job="shadow ? null : job" />
          <hr class="hairline">
          <JobControls :printer="name" :printer-state="state" :job="shadow ? null : job" :role="role" @changed="refresh" />
        </div>
      </div>
    </div>

    <FileBrowser :printer="name" :role="role" :printer-state="state" />
  </section>
</template>

<style scoped>
.back { margin: 0 0 var(--s3); }
.head { display: flex; align-items: center; gap: var(--s4); margin-bottom: var(--s4); }
.head h1 { margin: 0; overflow-wrap: anywhere; }
.head p { margin: 0; }
.id { flex: 1; min-width: 0; }
.cols { display: grid; gap: var(--s6); margin-bottom: var(--s6); }
.temps { display: grid; grid-template-columns: 1fr 1fr; gap: var(--s4); }
@media (min-width: 64rem) { .cols { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); } }
</style>
