<script setup lang="ts">
const props = defineProps<{
  open: boolean
  title: string
  body: string
  confirmLabel: string
  danger?: boolean
  busy?: boolean
  error?: string | null
}>()
const emit = defineEmits<{ confirm: []; cancel: [] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const cancelBtn = ref<HTMLButtonElement | null>(null)

function sync() {
  const d = dialog.value
  if (!d) return
  if (props.open && !d.open) {
    d.showModal()
    cancelBtn.value?.focus()
  } else if (!props.open && d.open) {
    d.close()
  }
}
onMounted(sync)
watch(() => props.open, sync)
</script>

<template>
  <dialog ref="dialog" aria-labelledby="confirm-title" @cancel.prevent="emit('cancel')">
    <h2 id="confirm-title">{{ title }}</h2>
    <p>{{ body }}</p>
    <p v-if="error" class="error-text" role="alert">{{ error }}</p>
    <div class="actions">
      <button ref="cancelBtn" type="button" class="btn btn-secondary" @click="emit('cancel')">Cancel</button>
      <button type="button" class="btn" :class="danger ? 'btn-danger' : 'btn-primary'" :disabled="busy" :aria-busy="busy" @click="emit('confirm')">{{ busy ? 'Working…' : confirmLabel }}</button>
    </div>
  </dialog>
</template>

<style scoped>
h2 { font-size: var(--step-1); }
.actions { display: flex; justify-content: flex-end; gap: var(--s3); margin-top: var(--s4); }
.error-text { color: var(--st-fault-ink); font-weight: 600; }
.error-text::before { content: '⚠ '; }
</style>
