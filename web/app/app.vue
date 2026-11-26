<script setup lang="ts">
const { message } = useFlash()
</script>

<template>
  <div>
    <p v-if="message" class="flash" role="status">{{ message }}</p>
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </div>
</template>
