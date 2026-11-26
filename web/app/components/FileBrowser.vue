<script setup lang="ts">
import type { FileEntry, Role, StorageDevice } from '~/types/api'
import { errorMessage } from '~/utils/api'
import { formatBytes, formatDate, formatDuration } from '~/utils/format'
import { filePath, printerPath } from '~/utils/paths'
import { roleAtLeast } from '~/utils/roles'

const props = defineProps<{ printer: string; role: Role; printerState: string }>()
const api = useApi()

const storages = ref<StorageDevice[]>([])
const storage = ref<string | null>(null)
const segments = ref<string[]>([])
const entry = ref<FileEntry | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)

const pending = ref<{ kind: 'print' | 'delete'; entry: FileEntry } | null>(null)
const busy = ref(false)
const actionError = ref<string | null>(null)

const storageId = (s: StorageDevice) => s.name ?? s.path.replace(/^\/+|\/+$/g, '')
const available = computed(() => storages.value.filter((s) => s.available))
const label = (s: StorageDevice) => s.name ?? s.path
const shown = (e: FileEntry) => e.display_name ?? e.name

const rows = computed(() => {
  const kids = [...(entry.value?.children ?? [])]
  return kids.sort((a, b) => {
    const fa = a.kind === 'folder' ? 0 : 1
    const fb = b.kind === 'folder' ? 0 : 1
    return fa - fb || shown(a).localeCompare(shown(b))
  })
})
const canPrint = computed(() => roleAtLeast(props.role, 'operator'))
const canDelete = computed(() => roleAtLeast(props.role, 'admin'))
const running = computed(() => ['PRINTING', 'PAUSED', 'ATTENTION', 'BUSY'].includes(props.printerState.toUpperCase()))

async function load() {
  if (!storage.value) return
  loading.value = true
  error.value = null
  try {
    entry.value = await api<FileEntry>(filePath(props.printer, storage.value, segments.value))
  } catch (e) {
    entry.value = null
    error.value = errorMessage(e)
  } finally {
    loading.value = false
  }
}

async function init() {
  try {
    storages.value = (await api<StorageDevice[]>(printerPath(props.printer, 'storage'))) ?? []
    const first = available.value[0]
    if (!first) {
      loading.value = false
      return
    }
    storage.value = storageId(first)
    await load()
  } catch (e) {
    error.value = errorMessage(e)
    loading.value = false
  }
}
onMounted(init)

function pick(s: StorageDevice) {
  storage.value = storageId(s)
  segments.value = []
  void load()
}
function open(name: string) {
  segments.value = [...segments.value, name]
  void load()
}
function crumb(depth: number) {
  segments.value = segments.value.slice(0, depth)
  void load()
}

function ask(kind: 'print' | 'delete', e: FileEntry) {
  actionError.value = null
  pending.value = { kind, entry: e }
}
async function confirm() {
  const p = pending.value
  if (!p || !storage.value) return
  busy.value = true
  actionError.value = null
  const path = [...segments.value, p.entry.name]
  try {
    if (p.kind === 'print') await api(filePath(props.printer, storage.value, path, 'print'), { method: 'POST' })
    else await api(filePath(props.printer, storage.value, path), { method: 'DELETE' })
    pending.value = null
    if (p.kind === 'delete') await load()
  } catch (e) {
    actionError.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

const dialog = computed(() => {
  const p = pending.value
  const name = p ? shown(p.entry) : ''
  const where = storages.value.find((s) => storageId(s) === storage.value)
  return p?.kind === 'delete'
    ? { title: `Delete ${name}?`, body: `This removes it from ${where ? label(where) : storage.value} on ${props.printer}. This can't be undone.`, confirm: 'Delete', danger: true }
    : { title: `Start printing ${name} on ${props.printer}?`, body: 'Make sure the bed is clear.', confirm: 'Start print', danger: false }
})
</script>

<template>
  <section class="files" aria-label="Files">
    <h2>Files</h2>
    <div v-if="available.length > 1" role="tablist" class="tabs" aria-label="Storage">
      <button v-for="s in available" :key="storageId(s)" type="button" role="tab" class="btn btn-small" :class="storageId(s) === storage ? 'btn-primary' : 'btn-secondary'" :aria-selected="storageId(s) === storage" @click="pick(s)">{{ label(s) }}</button>
    </div>
    <div v-else-if="available.length === 1" role="tablist" class="tabs" aria-label="Storage">
      <span role="tab" class="chip" aria-selected="true">{{ label(available[0]!) }}</span>
    </div>

    <nav v-if="storage" aria-label="Folder" class="crumbs">
      <button type="button" class="link" @click="crumb(0)">{{ storage }}</button>
      <template v-for="(seg, i) in segments" :key="i">
        <span aria-hidden="true">/</span>
        <button type="button" class="link" @click="crumb(i + 1)">{{ seg }}</button>
      </template>
    </nav>

    <Notice v-if="error" kind="error">{{ error }}</Notice>
    <p v-if="!loading && !available.length && !error" class="muted">No storage is available on this printer.</p>
    <p v-else-if="loading" class="muted">Loading files…</p>

    <div v-else-if="entry && !error" class="table-wrap">
      <p v-if="!rows.length" class="muted">This folder is empty. Upload files with `starstar files upload`.</p>
      <table v-else>
        <thead>
          <tr><th scope="col">Name</th><th scope="col">Kind</th><th scope="col">Size</th><th scope="col">Modified</th><th scope="col">Print time</th><th scope="col">Filament</th><th scope="col"><span class="visually-hidden">Actions</span></th></tr>
        </thead>
        <tbody>
          <tr v-for="e in rows" :key="e.name">
            <th scope="row" class="name">
              <button v-if="e.kind === 'folder'" type="button" class="link" @click="open(e.name)">{{ shown(e) }}</button>
              <template v-else>{{ shown(e) }}</template>
            </th>
            <td>{{ e.kind === 'folder' ? 'Folder' : e.kind === 'print_file' ? 'Print file' : e.kind === 'firmware' ? 'Firmware' : 'File' }}</td>
            <td class="num">{{ e.kind === 'folder' ? '' : formatBytes(e.size) }}</td>
            <td class="num">{{ e.modified ? formatDate(e.modified) : '' }}</td>
            <td class="num">{{ e.kind === 'print_file' ? formatDuration(e.meta?.estimated_print_time) : '' }}</td>
            <td>{{ e.kind === 'print_file' ? (e.meta?.filament_type ?? '—') : '' }}</td>
            <td class="actions">
              <button v-if="canPrint && e.kind === 'print_file'" type="button" class="btn btn-small btn-primary" :disabled="running" :title="running ? 'A job is already running.' : undefined" @click="ask('print', e)">Print</button>
              <button v-if="canDelete && e.kind !== 'folder'" type="button" class="btn btn-small btn-secondary" @click="ask('delete', e)">Delete</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <ConfirmDialog
      :open="!!pending"
      :title="dialog.title"
      :body="dialog.body"
      :confirm-label="dialog.confirm"
      :danger="dialog.danger"
      :busy="busy"
      :error="actionError"
      @confirm="confirm"
      @cancel="pending = null"
    />
  </section>
</template>

<style scoped>
.tabs { display: flex; gap: var(--s2); margin-bottom: var(--s3); }
.crumbs { display: flex; flex-wrap: wrap; gap: var(--s2); align-items: center; margin-bottom: var(--s3); }
.link { font: inherit; background: none; border: 0; padding: 0; color: inherit; text-decoration: underline; cursor: pointer; font-weight: 600; }
.name { font-weight: 400; overflow-wrap: anywhere; min-width: 9rem; }
.actions { white-space: nowrap; display: flex; gap: var(--s2); }
</style>
