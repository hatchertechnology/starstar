<script setup lang="ts">
import { errorMessage } from '~/utils/api'
import { formatRelative } from '~/utils/format'
import { printerPath } from '~/utils/paths'

const props = defineProps<{ printer: string }>()
const api = useApi()

const url = ref<string | null>(null)
const takenAt = ref<number | null>(null)
const none = ref(false)
const error = ref<string | null>(null)
const loading = ref(false)
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | undefined

function release() {
  if (url.value) URL.revokeObjectURL(url.value)
  url.value = null
}

async function load() {
  loading.value = true
  error.value = null
  try {
    const blob = await api<Blob>(printerPath(props.printer, 'cameras', 'snap'), { as: 'blob' })
    if (!blob) {
      none.value = true
      release()
    } else {
      none.value = false
      const next = URL.createObjectURL(blob)
      release()
      url.value = next
      takenAt.value = Math.floor(Date.now() / 1000)
      now.value = Date.now()
    }
  } catch (e) {
    error.value = errorMessage(e)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void load()
  clock = setInterval(() => { now.value = Date.now() }, 5000)
})
onBeforeUnmount(() => {
  clearInterval(clock)
  release()
})
</script>

<template>
  <figure class="snap">
    <div class="frame tool">
      <img v-if="url" :src="url" :alt="`Latest camera snapshot of ${printer}`">
      <p v-else-if="error" class="msg" role="alert">{{ error }}</p>
      <p v-else-if="none" class="msg muted">No snapshot yet. The camera takes them while printing.</p>
      <p v-else class="msg muted">Loading snapshot…</p>
    </div>
    <figcaption class="cap">
      <span v-if="url && takenAt" class="muted small">Taken {{ formatRelative(takenAt, now) }}</span>
      <button type="button" class="btn btn-small btn-secondary" :disabled="loading" @click="load">{{ error ? 'Try again' : 'Refresh snapshot' }}</button>
    </figcaption>
  </figure>
</template>

<style scoped>
.snap { margin: 0; }
.frame { aspect-ratio: 4 / 3; padding: 0; display: grid; place-items: center; overflow: hidden; }
.frame img { width: 100%; height: 100%; object-fit: contain; display: block; }
.msg { margin: 0; padding: var(--s4); text-align: center; }
.cap { display: flex; justify-content: space-between; align-items: center; gap: var(--s3); margin-top: var(--s2); }
</style>
