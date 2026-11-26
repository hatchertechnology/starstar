<script setup lang="ts">
withDefaults(defineProps<{ kind?: 'info' | 'error' }>(), { kind: 'info' })
</script>

<template>
  <div class="notice" :class="kind === 'error' ? 'notice-error' : ''" :role="kind === 'error' ? 'alert' : 'status'">
    <slot />
  </div>
</template>
