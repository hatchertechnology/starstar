<script setup lang="ts">
import type { PartFile } from "~/types/api";
import { filesPath } from "~/utils/paths";
import { formatBytes } from "~/utils/format";
defineProps<{ files: PartFile[]; admin: boolean; busy: boolean }>();
defineEmits<{ delete: [file: PartFile] }>();
</script>
<template>
  <ul v-if="files.length">
    <li v-for="file in files" :key="file.id">
      <img
        v-if="file.role === 'render'"
        :src="filesPath(file.id, 'preview')"
        :alt="file.name"
        class="render"
      /><a :href="filesPath(file.id, 'download')">{{ file.name }}</a> —
      {{ file.role }} · {{ formatBytes(file.size) }}
      <button
        v-if="admin"
        type="button"
        class="btn btn-secondary"
        :disabled="busy"
        @click="$emit('delete', file)"
      >
        Delete file
      </button>
    </li>
  </ul>
  <p v-else class="muted">No files.</p>
</template>
<style scoped>
.render {
  display: block;
  max-width: 12rem;
  max-height: 10rem;
  object-fit: contain;
}
</style>
