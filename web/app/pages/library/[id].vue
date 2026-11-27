<script setup lang="ts">
import type { LibraryFile } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'
import { libraryPath } from '~/utils/paths'
const api = useApi()
const session = useSession()
const id = Number(useRoute().params.id)
const file = ref<LibraryFile | null>(null)
const preview = ref<string | null>(null)
const noPreview = ref(false)
const gone = ref(false)
const error = ref<string | null>(null)
const actionError = ref<string | null>(null)
const pending = ref(false)
const busy = ref(false)
let unmounted = false
useHead({ title: computed(() => `${file.value?.name ?? 'Library'} · starstar`) })
function release() { if (preview.value) URL.revokeObjectURL(preview.value); preview.value = null }
async function load() {
  try {
    file.value = await api<LibraryFile>(libraryPath(id))
    if (unmounted) return
    if (!file.value?.has_preview) { noPreview.value = true; return }
    try {
      const blob = await api<Blob>(libraryPath(id, 'preview'), { as: 'blob' })
      if (unmounted) return
      if (!blob) throw new Error('empty')
      preview.value = URL.createObjectURL(blob)
    } catch { noPreview.value = true }
  } catch (e) { if (e instanceof ApiRequestError && e.status === 404) gone.value = true; else error.value = errorMessage(e) }
}
onMounted(load)
onBeforeUnmount(() => { unmounted = true; release() })
async function remove() {
  busy.value = true; actionError.value = null
  try { await api(libraryPath(id), { method: 'DELETE' }); pending.value = false; await navigateTo('/library') }
  catch (e) { actionError.value = errorMessage(e) }
  finally { busy.value = false }
}
</script>
<template>
  <section>
    <p class="back small"><NuxtLink to="/library">← Library</NuxtLink></p>
    <template v-if="gone"><h1>Library</h1><Notice kind="error">This file is not in the library.</Notice></template>
    <template v-else><h1 v-if="file">{{ file.name }} v{{ file.version }}</h1><Notice v-if="error" kind="error">{{ error }}</Notice>
      <div v-if="file" class="cols"><figure class="preview"><img v-if="preview" :src="preview" :alt="`Preview of ${file.name}`" @error="release(); noPreview = true"><p v-else-if="noPreview" class="muted">No preview in this file</p><p v-else class="muted">Loading preview…</p></figure>
        <div><Notice v-if="!file.parsed" kind="info">This file's slicer details could not be read.</Notice><PrintFacts :meta="file.meta" :size="file.size" /><div class="actions"><a class="btn btn-primary" :href="libraryPath(id, 'download')">Download</a><button v-if="session.can('admin')" class="btn btn-secondary" type="button" @click="pending = true; actionError = null">Delete</button></div></div></div>
    </template>
    <ConfirmDialog :open="pending" :title="`Delete ${file?.name} v${file?.version}?`" body="This removes it from the library. Copies already on printers are not affected." confirm-label="Delete" danger :busy="busy" :error="actionError" @confirm="remove" @cancel="pending = false" />
  </section>
</template>
<style scoped>
.back { margin: 0 0 var(--s3); } h1 { overflow-wrap: anywhere; } .cols { display: grid; gap: var(--s6); } .preview { margin: 0; } .preview img { display: block; width: 100%; max-width: 30rem; height: auto; } .actions { display: flex; gap: var(--s3); } @media (min-width: 64rem) { .cols { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); } }
</style>
