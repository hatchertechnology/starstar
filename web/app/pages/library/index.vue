<script setup lang="ts">
import type { Part, PartSummary, PartSort } from '~/types/api'
import { errorMessage } from '~/utils/api'
import { formatDate } from '~/utils/format'
import { filesPath, partsPath } from '~/utils/paths'

const api = useApi()
const session = useSession()
const parts = ref<PartSummary[]>([])
const search = ref('')
const sort = ref<PartSort>('newest')
const listError = ref<string | null>(null)
const createError = ref<string | null>(null)
const creating = ref(false)
const showNew = ref(false)
const name = ref('')
let request = 0
useHead({ title: 'Library · starstar' })
async function load() {
  const current = ++request
  try {
    const result = await api<PartSummary[]>(partsPath(), { query: { q: search.value || undefined, sort: sort.value === 'newest' ? undefined : sort.value } })
    if (current === request) { parts.value = result ?? []; listError.value = null }
  } catch (e) { if (current === request) listError.value = errorMessage(e) }
}
watch([search, sort], load)
onMounted(load)
async function create() {
  if (creating.value || !name.value.trim()) return
  creating.value = true
  createError.value = null
  try {
    const result = await api<Part>(partsPath(), { method: 'POST', json: { name: name.value.trim() } })
    if (result) await navigateTo(`/library/${result.id}`)
  } catch (e) { createError.value = errorMessage(e) }
  finally { creating.value = false }
}
</script>

<template>
  <section>
    <div class="heading"><h1>Library</h1><button v-if="session.can('operator') && !showNew" class="btn btn-primary" type="button" @click="showNew = true">New part</button></div>
    <form v-if="session.can('operator') && showNew" class="new-part" @submit.prevent="create"><label>Name <input v-model="name" name="name" :disabled="creating" required></label><button class="btn btn-primary" type="submit" :disabled="creating || !name.trim()">{{ creating ? 'Creating…' : 'Create' }}</button><button class="btn btn-secondary" type="button" :disabled="creating" @click="showNew = false; createError = null">Cancel</button></form>
    <Notice v-if="listError" kind="error">{{ listError }}</Notice>
    <Notice v-if="createError" kind="error">{{ createError }}</Notice>
    <div class="filters"><label>Search <input v-model="search" type="search"></label><label>Sort <select v-model="sort"><option value="newest">Newest</option><option value="name">Name</option></select></label></div>
    <div class="table-wrap"><table><thead><tr><th>Preview</th><th>Name</th><th>Model rev</th><th>Setups</th><th>Slices</th><th>Last change</th></tr></thead><tbody><tr v-for="part in parts" :key="part.id"><td><img v-if="part.preview_file_id !== null" class="preview" :src="filesPath(part.preview_file_id, 'preview')" alt=""><span v-else class="muted">—</span></td><td><NuxtLink :to="`/library/${part.id}`">{{ part.name }}</NuxtLink></td><td>{{ part.latest_model_revision }}</td><td>{{ part.setups }}</td><td>{{ part.slices }}</td><td>{{ formatDate(part.updated_at) }}</td></tr></tbody></table></div>
    <p v-if="!parts.length && !listError" class="muted">No parts in the library.</p>
  </section>
</template>
<style scoped>
.heading { display: flex; justify-content: space-between; align-items: center; gap: var(--s3); }
.filters, .new-part { display: flex; flex-wrap: wrap; gap: var(--s4); margin-bottom: var(--s4); }
.filters label, .new-part label { display: grid; gap: var(--s1); }
.new-part { align-items: end; }
.preview { width: 48px; height: 48px; object-fit: contain; }
</style>
