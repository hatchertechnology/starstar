<script setup lang="ts">
const props = defineProps<{ id: string; label: string; hint?: string; error?: string | null }>()
const describedby = computed(() => [props.hint ? `${props.id}-hint` : '', props.error ? `${props.id}-error` : ''].filter(Boolean).join(' ') || undefined)
</script>

<template>
  <div class="field">
    <label :for="id">{{ label }}</label>
    <slot :describedby="describedby" :invalid="!!error" />
    <span v-if="hint" :id="`${id}-hint`" class="hint">{{ hint }}</span>
    <span v-if="error" :id="`${id}-error`" class="error" role="alert">{{ error }}</span>
  </div>
</template>
