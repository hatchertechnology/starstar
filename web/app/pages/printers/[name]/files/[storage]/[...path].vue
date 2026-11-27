<script setup lang="ts">
import type { FileEntry, PrinterInfo, PrinterSummary } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'
import { slicedMismatch } from '~/utils/compat'
import { formatBytes, formatDate, formatDuration } from '~/utils/format'
import { filePath, folderQuery, printerPath, thumbnailPath } from '~/utils/paths'
import { roleAtLeast } from '~/utils/roles'

const route = useRoute()
const api = useApi()
const session = useSession()

const name = computed(() => String(route.params.name))
const storage = computed(() => String(route.params.storage))
const segments = computed(() => {
  const p = route.params.path
  return (Array.isArray(p) ? p : [p]).filter((s): s is string => !!s)
})
const backTo = computed(() => ({ path: `/printers/${encodeURIComponent(name.value)}`, query: folderQuery(storage.value, segments.value.slice(0, -1)) }))

const file = ref<FileEntry | null>(null)
const gone = ref(false)
const loadError = ref<string | null>(null)
const printer = ref<PrinterSummary | null>(null)
const info = ref<PrinterInfo | null>(null)
const printingNow = ref<boolean | null>(null)
const preview = ref<string | null>(null)
const noPreview = ref(false)

const shown = computed(() => file.value?.display_name ?? file.value?.name ?? segments.value.at(-1) ?? '')
useHead({ title: computed(() => `${shown.value} · ${name.value} · starstar`) })

const { status } = usePrinterPoll(name.value)
const role = computed(() => session.user.value?.role ?? 'viewer')
const canPrint = computed(() => roleAtLeast(role.value, 'operator'))
const canDelete = computed(() => roleAtLeast(role.value, 'admin'))
const running = computed(() => ['PRINTING', 'PAUSED', 'ATTENTION', 'BUSY'].includes((status.value?.state ?? '').toUpperCase()))
const mismatch = computed(() => (printer.value ? slicedMismatch(file.value?.meta, printer.value.kind, info.value?.nozzle_diameter, name.value) : null))

const pending = ref<'print' | 'delete' | null>(null)
const busy = ref(false)
const actionError = ref<string | null>(null)
const dialog = computed(() => pending.value === 'delete'
  ? { title: `Delete ${shown.value}?`, body: `This removes it from ${storage.value} on ${name.value}. This can't be undone.`, confirm: 'Delete', danger: true }
  : { title: `Start printing ${shown.value} on ${name.value}?`, body: 'Make sure the bed is clear.', confirm: 'Start print', danger: false })

function release() {
  if (preview.value) URL.revokeObjectURL(preview.value)
  preview.value = null
}

async function loadPreview() {
  if (!file.value?.has_thumbnail) {
    noPreview.value = true
    return
  }
  try {
    const blob = await api<Blob>(thumbnailPath(name.value, storage.value, segments.value), { as: 'blob' })
    if (!blob) throw new Error('empty')
    release()
    preview.value = URL.createObjectURL(blob)
  } catch {
    noPreview.value = true
  }
}

async function load() {
  const path = filePath(name.value, storage.value, segments.value)
  try {
    file.value = await api<FileEntry>(path)
  } catch (e) {
    if (e instanceof ApiRequestError && e.status === 404) gone.value = true
    else loadError.value = errorMessage(e)
    return
  }
  void loadPreview()
  api<Headers>(path, { method: 'HEAD', as: 'headers' })
    .then((h) => { printingNow.value = h?.get('currently-printed') === 'true' })
    .catch(() => { printingNow.value = null })
  api<PrinterSummary>(printerPath(name.value)).then((p) => { printer.value = p }).catch(() => {})
  api<PrinterInfo>(printerPath(name.value, 'info')).then((i) => { info.value = i }).catch(() => {})
}
onMounted(load)
onBeforeUnmount(release)

async function confirm() {
  const kind = pending.value
  if (!kind) return
  busy.value = true
  actionError.value = null
  try {
    if (kind === 'print') await api(filePath(name.value, storage.value, segments.value, 'print'), { method: 'POST' })
    else await api(filePath(name.value, storage.value, segments.value), { method: 'DELETE' })
    pending.value = null
    if (kind === 'delete') await navigateTo(backTo.value)
  } catch (e) {
    actionError.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

const m = computed(() => file.value?.meta ?? null)
</script>

<template>
  <section class="detail">
    <p class="back small"><NuxtLink :to="backTo">← {{ name }}</NuxtLink></p>

    <template v-if="gone">
      <h1>{{ shown }}</h1>
      <Notice kind="error">This file is no longer on the printer.</Notice>
    </template>

    <template v-else>
      <h1>{{ shown }}</h1>
      <Notice v-if="loadError" kind="error">{{ loadError }}</Notice>

      <div v-if="file" class="cols">
        <figure class="preview">
          <img v-if="preview" :src="preview" :alt="`Preview of ${shown}`">
          <p v-else-if="noPreview" class="msg muted">No preview in this file</p>
          <p v-else class="msg muted">Loading preview…</p>
        </figure>

        <div class="stack">
          <Notice v-if="mismatch" kind="info">{{ mismatch }}</Notice>
          <dl class="facts">
            <template v-if="m?.printer_model"><dt>Sliced for</dt><dd>{{ m.printer_model }}</dd></template>
            <template v-if="m?.nozzle_diameter != null"><dt>Nozzle</dt><dd>{{ m.nozzle_diameter }} mm</dd></template>
            <template v-if="m?.layer_height != null"><dt>Layer height</dt><dd>{{ m.layer_height }} mm</dd></template>
            <template v-if="m?.filament_type"><dt>Filament</dt><dd>{{ m.filament_type }}<template v-if="m.filament_used_g != null"> · {{ m.filament_used_g }} g</template></dd></template>
            <template v-if="m?.nozzle_temp != null || m?.bed_temp != null"><dt>Temperatures</dt><dd>nozzle {{ m?.nozzle_temp ?? '—' }} °C · bed {{ m?.bed_temp ?? '—' }} °C</dd></template>
            <template v-if="m?.estimated_print_time != null"><dt>Estimated time</dt><dd>{{ formatDuration(m.estimated_print_time) }}</dd></template>
            <dt>Size</dt><dd>{{ formatBytes(file.size) }}</dd>
            <template v-if="file.modified"><dt>Modified</dt><dd>{{ formatDate(file.modified) }}</dd></template>
            <template v-if="printingNow !== null"><dt>Printing now</dt><dd>{{ printingNow ? 'yes' : 'no' }}</dd></template>
          </dl>

          <div v-if="canPrint || canDelete" class="actions">
            <button v-if="canPrint" type="button" class="btn btn-primary" :disabled="running" :title="running ? 'A job is already running.' : undefined" @click="pending = 'print'; actionError = null">Print</button>
            <button v-if="canDelete" type="button" class="btn btn-secondary" @click="pending = 'delete'; actionError = null">Delete</button>
          </div>
        </div>
      </div>
    </template>

    <ConfirmDialog :open="pending !== null" :title="dialog.title" :body="dialog.body" :confirm-label="dialog.confirm" :danger="dialog.danger" :busy="busy" :error="actionError" @confirm="confirm" @cancel="pending = null" />
  </section>
</template>

<style scoped>
.back { margin: 0 0 var(--s3); }
h1 { overflow-wrap: anywhere; }
.cols { display: grid; gap: var(--s6); }
.preview { margin: 0; }
.preview img { display: block; width: 100%; max-width: 30rem; height: auto; }
.facts { display: grid; grid-template-columns: max-content 1fr; gap: var(--s2) var(--s4); margin: 0 0 var(--s4); }
.facts dt { color: var(--muted, inherit); }
.facts dd { margin: 0; }
.actions { display: flex; gap: var(--s3); }
@media (min-width: 64rem) { .cols { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); } }
</style>
