<script setup lang="ts">
import type { Job, Role } from '~/types/api'
import { errorMessage } from '~/utils/api'
import { formatPercent } from '~/utils/format'
import { jobActions, type JobAction } from '~/utils/jobs'
import { printerPath } from '~/utils/paths'
import { roleAtLeast } from '~/utils/roles'

const props = defineProps<{ printer: string; printerState: string; job: Job | null; role: Role }>()
const emit = defineEmits<{ changed: [] }>()

const api = useApi()
const labels: Record<JobAction, string> = { pause: 'Pause', resume: 'Resume', continue: 'Continue', stop: 'Stop' }
const actions = computed(() => jobActions(props.printerState, props.job))
const operator = computed(() => roleAtLeast(props.role, 'operator'))

const busy = ref(false)
const error = ref<string | null>(null)
const confirming = ref(false)
const stopError = ref<string | null>(null)

async function send(action: JobAction): Promise<void> {
  if (!props.job) return
  const id = String(props.job.id)
  if (action === 'stop') await api(printerPath(props.printer, 'job', id), { method: 'DELETE' })
  else await api(printerPath(props.printer, 'job', id, action), { method: 'PUT' })
}

async function run(action: JobAction) {
  if (action === 'stop') {
    stopError.value = null
    confirming.value = true
    return
  }
  busy.value = true
  error.value = null
  try {
    await send(action)
    emit('changed')
  } catch (e) {
    error.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

async function confirmStop() {
  busy.value = true
  stopError.value = null
  try {
    await send('stop')
    confirming.value = false
    emit('changed')
  } catch (e) {
    stopError.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

const fileName = computed(() => props.job?.file?.display_name ?? props.job?.file?.name ?? 'The print')
</script>

<template>
  <div class="controls">
    <p v-if="!operator" class="muted">Operators can pause and stop prints.</p>
    <template v-else>
      <Notice v-if="error" kind="error">{{ error }}</Notice>
      <div v-if="actions.length" class="row">
        <button
          v-for="a in actions"
          :key="a"
          type="button"
          class="btn"
          :class="a === 'stop' ? 'btn-secondary' : 'btn-primary'"
          :disabled="busy"
          @click="run(a)"
        >{{ labels[a] }}</button>
      </div>
      <p v-else class="muted small">No controls while the printer is {{ printerState.toLowerCase() }}.</p>
    </template>
    <ConfirmDialog
      :open="confirming"
      :title="`Stop the print on ${printer}?`"
      :body="`${fileName} is ${formatPercent(job?.progress ?? 0)} done. A stopped print can't be resumed.`"
      confirm-label="Stop print"
      danger
      :busy="busy"
      :error="stopError"
      @confirm="confirmStop"
      @cancel="confirming = false"
    />
  </div>
</template>

<style scoped>
.row { display: flex; gap: var(--s3); flex-wrap: wrap; }
</style>
