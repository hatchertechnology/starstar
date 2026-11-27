<script setup lang="ts">
import type { LibraryFile } from '~/types/api'
import { errorMessage } from '~/utils/api'
import { formatDate, formatDuration } from '~/utils/format'
import { libraryPath } from '~/utils/paths'

const api = useApi()
const session = useSession()
const files = ref<LibraryFile[]>([])
const search = ref('')
const sort = ref('newest')
const listError = ref<string | null>(null)
const uploadError = ref<string | null>(null)
const uploading = ref(false)
const picker = ref<HTMLInputElement | null>(null)
let request = 0
useHead({ title: 'Library · starstar' })
async function load() {
  const current = ++request
  try {
    const result = await api<LibraryFile[]>(libraryPath(), { query: { q: search.value || undefined, sort: sort.value === 'newest' ? undefined : sort.value } })
    if (current === request) { files.value = result ?? []; listError.value = null }
  } catch (e) { if (current === request) listError.value = errorMessage(e) }
}
watch([search, sort], load)
onMounted(load)
async function upload(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  uploading.value = true
  uploadError.value = null
  try {
    const result = await api<LibraryFile>(libraryPath(), { method: 'POST', query: { name: file.name }, body: file })
    if (result) await navigateTo(`/library/${result.id}`)
  } catch (e) { uploadError.value = errorMessage(e) }
  finally { uploading.value = false; input.value = '' }
}
</script>

<template>
  <section>
    <div class="heading"><h1>Library</h1><div v-if="session.can('operator')"><button class="btn btn-primary" type="button" :disabled="uploading" @click="picker?.click()">{{ uploading ? 'Uploading…' : 'Upload' }}</button><input ref="picker" class="visually-hidden" type="file" accept=".bgcode,.gcode" tabindex="-1" @change="upload"></div></div>
    <Notice v-if="listError" kind="error">{{ listError }}</Notice>
    <Notice v-if="uploadError" kind="error">{{ uploadError }}</Notice>
    <div class="filters"><label>Search <input v-model="search" type="search"></label><label>Sort <select v-model="sort"><option value="newest">Newest</option><option value="name">Name</option><option value="sliced_for">Sliced for</option></select></label></div>
    <div class="table-wrap"><table><thead><tr><th>Name</th><th>Version</th><th>Sliced for</th><th>Filament</th><th>Est. time</th><th>Uploaded</th></tr></thead><tbody><tr v-for="file in files" :key="file.id"><td><NuxtLink :to="`/library/${file.id}`">{{ file.name }}</NuxtLink></td><td>{{ file.version }}</td><td>{{ file.meta?.printer_model ?? '—' }}</td><td>{{ file.meta?.filament_type ?? '—' }}</td><td>{{ file.meta?.estimated_print_time != null ? formatDuration(file.meta.estimated_print_time) : '—' }}</td><td>{{ formatDate(file.uploaded_at) }}</td></tr></tbody></table></div>
    <p v-if="!files.length && !listError" class="muted">No files in the library.</p>
  </section>
</template>
<style scoped>
.heading { display: flex; justify-content: space-between; align-items: center; gap: var(--s3); }
.filters { display: flex; flex-wrap: wrap; gap: var(--s4); margin-bottom: var(--s4); }
.filters label { display: grid; gap: var(--s1); }
</style>
