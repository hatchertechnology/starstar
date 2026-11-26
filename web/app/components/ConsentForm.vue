<script setup lang="ts">
import type { ConsentRequest, Role } from '~/types/api'
import { ROLES, roleMin } from '~/utils/roles'

const props = defineProps<{ request: ConsentRequest; csrf: string }>()

const ACTION = '/oauth/authorize/decision'
const describe: Record<string, string> = {
  viewer: 'See printers, jobs, files and snapshots.',
  operator: 'Also upload, start, pause, resume and stop prints.',
}

const choices = computed<Role[]>(() => {
  const ceiling = roleMin(props.request.max_role, 'operator')
  return ROLES.filter((r) => ROLES.indexOf(r) <= ROLES.indexOf(ceiling))
})
const canAdmin = computed(() => props.request.max_role === 'admin')
const radio = ref<Role>(roleMin(props.request.default_role, 'operator'))
const admin = ref(false)
</script>

<template>
  <div class="stack">
    <form method="post" :action="ACTION" class="stack" data-testid="allow-form">
      <input type="hidden" name="request" :value="request.id">
      <input type="hidden" name="csrf_token" :value="csrf">
      <!-- While the admin box is ticked the radios are disabled, so the only role posted is this one. -->
      <template v-if="admin">
        <input type="hidden" name="role" value="admin">
        <input type="hidden" name="allow_admin" value="true">
      </template>

      <fieldset class="levels">
        <legend>Access level</legend>
        <label v-for="r in choices" :key="r" class="choice">
          <input v-model="radio" type="radio" name="role" :value="r" :disabled="admin">
          <span><strong>{{ r }}</strong><br><span class="muted small">{{ describe[r] }}</span></span>
        </label>
        <p v-if="request.max_role === 'viewer'" class="muted">Your account is a viewer, so this app can only view.</p>
      </fieldset>

      <label v-if="canAdmin" class="choice">
        <input v-model="admin" type="checkbox">
        <span>Also allow admin actions (delete files, manage printers, cameras, firmware and users)</span>
      </label>

      <button class="btn btn-primary" type="submit" name="decision" value="allow">Allow</button>
    </form>

    <form method="post" :action="ACTION" data-testid="deny-form">
      <input type="hidden" name="request" :value="request.id">
      <input type="hidden" name="csrf_token" :value="csrf">
      <button class="btn btn-secondary" type="submit" name="decision" value="deny">Deny</button>
    </form>
  </div>
</template>

<style scoped>
.levels { border: 0; padding: 0; margin: 0; display: grid; gap: var(--s3); }
.levels legend { font-weight: 600; margin-bottom: var(--s2); padding: 0; }
.choice { display: flex; gap: var(--s3); align-items: flex-start; }
.choice input { margin-top: 0.2rem; }
</style>
