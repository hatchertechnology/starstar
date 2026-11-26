<script setup lang="ts">
// One geometry per kind: the filled glyph, the offline shadow and the empty-board place all use it.
const props = defineProps<{ kind: 'mk4' | 'mini' | 'unknown'; variant: 'filled' | 'shadow' }>()
const shapes = {
  mk4: { body: 'M8 42V6h32v36z', detail: 'M8 15h32M17 15v6h14v-6M13 36h22' },
  mini: { body: 'M10 42V12h14v8h14v22z', detail: 'M24 20v-8M14 36h20' },
  unknown: { body: 'M8 8h32v32H8z', detail: 'M8 32h32' },
} as const
const shape = computed(() => shapes[props.kind])
</script>

<template>
  <svg class="silhouette" :class="`is-${variant}`" viewBox="0 0 48 48" aria-hidden="true" focusable="false">
    <path class="body" :d="shape.body" />
    <path class="detail" :d="shape.detail" />
  </svg>
</template>

<style scoped>
.silhouette { width: 2.5rem; height: 2.5rem; flex: none; color: var(--ink); }
.body, .detail { stroke: currentColor; stroke-width: 2.5; stroke-linejoin: round; stroke-linecap: round; fill: none; transition: fill-opacity var(--t-state) var(--ease); }
.is-filled .body { fill: currentColor; fill-opacity: 0.14; }
.is-shadow .body, .is-shadow .detail { stroke-dasharray: 3 3; }
</style>
