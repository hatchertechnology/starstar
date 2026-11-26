<script setup lang="ts">
import type { NewUser, Role, UpdateUser, UserSummary } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'
import { formatDate } from '~/utils/format'
import { ROLES } from '~/utils/roles'

definePageMeta({ minRole: 'admin' })
useHead({ title: 'Users · starstar' })

const api = useApi()
const { user: me } = useSession()
const users = ref<UserSummary[] | null>(null)
const listError = ref<string | null>(null)
const rowError = reactive<Record<string, string>>({})
const busy = reactive<Record<string, boolean>>({})

async function load() {
  listError.value = null
  try {
    users.value = (await api<UserSummary[]>('/api/users')) ?? []
  } catch (e) {
    listError.value = errorMessage(e)
  }
}
onMounted(load)

async function patch(u: UserSummary, body: UpdateUser): Promise<boolean> {
  busy[u.username] = true
  delete rowError[u.username]
  try {
    const updated = await api<UserSummary>(`/api/users/${encodeURIComponent(u.username)}`, { method: 'PATCH', json: body })
    if (updated) Object.assign(u, updated)
    return true
  } catch (e) {
    rowError[u.username] = errorMessage(e)
    return false
  } finally {
    busy[u.username] = false
  }
}

async function changeRole(u: UserSummary, ev: Event) {
  const select = ev.target as HTMLSelectElement
  const ok = await patch(u, { role: select.value as Role })
  if (!ok) select.value = u.role
}

const removing = ref<UserSummary | null>(null)
const removeBusy = ref(false)
const removeError = ref<string | null>(null)

async function remove() {
  const u = removing.value
  if (!u) return
  removeBusy.value = true
  removeError.value = null
  try {
    await api(`/api/users/${encodeURIComponent(u.username)}`, { method: 'DELETE' })
    users.value = (users.value ?? []).filter((x) => x.username !== u.username)
    removing.value = null
  } catch (e) {
    removeError.value = errorMessage(e)
  } finally {
    removeBusy.value = false
  }
}

const form = reactive({ username: '', password: '', role: 'viewer' as Role })
const adding = ref(false)
const addError = ref<string | null>(null)
const added = ref(false)

async function add() {
  adding.value = true
  addError.value = null
  added.value = false
  const body: NewUser = { username: form.username, password: form.password, role: form.role }
  try {
    await api('/api/users', { method: 'POST', json: body })
    form.username = ''
    form.password = ''
    form.role = 'viewer'
    added.value = true
    await load()
  } catch (e) {
    form.password = ''
    addError.value = e instanceof ApiRequestError && e.status === 409 ? 'That username is taken.' : errorMessage(e)
  } finally {
    adding.value = false
  }
}
</script>

<template>
  <section class="stack">
    <h1>Users</h1>
    <Notice v-if="listError" kind="error">{{ listError }} <button type="button" class="btn btn-small btn-secondary" @click="load">Try again</button></Notice>

    <div class="tool table-wrap">
      <table>
        <thead>
          <tr><th scope="col">Username</th><th scope="col">Role</th><th scope="col">Status</th><th scope="col">Created</th><th scope="col"><span class="visually-hidden">Actions</span></th></tr>
        </thead>
        <tbody>
          <tr v-for="u in users ?? []" :key="u.username">
            <th scope="row">
              {{ u.username }}<template v-if="u.username === me?.username"> (you)</template>
              <span v-if="rowError[u.username]" class="row-error" role="alert">{{ rowError[u.username] }}</span>
            </th>
            <td>
              <select :aria-label="`Role for ${u.username}`" :value="u.role" :disabled="busy[u.username]" @change="changeRole(u, $event)">
                <option v-for="r in ROLES" :key="r" :value="r">{{ r }}</option>
              </select>
            </td>
            <td>{{ u.disabled ? 'Disabled' : 'Active' }}</td>
            <td class="num">{{ formatDate(u.created_at) }}</td>
            <td class="actions">
              <button type="button" class="btn btn-small btn-secondary" :disabled="busy[u.username]" @click="patch(u, { disabled: !u.disabled })">{{ u.disabled ? 'Enable' : 'Disable' }}</button>
              <button type="button" class="btn btn-small btn-secondary" @click="removeError = null; removing = u">Remove</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <form class="tool add" @submit.prevent="add">
      <h2>Add a user</h2>
      <Notice v-if="addError" kind="error">{{ addError }}</Notice>
      <Notice v-if="added">Share the password with them privately; they can change it under Account.</Notice>
      <FieldRow id="new-username" label="Username" hint="Lowercase letters, digits, dot, dash and underscore.">
        <template #default="{ describedby }">
          <input id="new-username" v-model="form.username" autocomplete="off" autocapitalize="none" spellcheck="false" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="new-password" label="Temporary password">
        <template #default="{ describedby }">
          <input id="new-password" v-model="form.password" type="password" autocomplete="new-password" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="new-role" label="Role">
        <template #default>
          <select id="new-role" v-model="form.role"><option v-for="r in ROLES" :key="r" :value="r">{{ r }}</option></select>
        </template>
      </FieldRow>
      <button class="btn btn-primary" type="submit" :disabled="adding" :aria-busy="adding">{{ adding ? 'Adding…' : 'Add user' }}</button>
    </form>

    <ConfirmDialog
      :open="!!removing"
      :title="`Remove ${removing?.username ?? ''}?`"
      body="Their sign-ins and connected apps stop working immediately."
      confirm-label="Remove"
      danger
      :busy="removeBusy"
      :error="removeError"
      @confirm="remove"
      @cancel="removing = null"
    />
  </section>
</template>

<style scoped>
.row-error { display: block; font-weight: 400; font-size: var(--step--1); color: var(--st-fault-ink); }
.row-error::before { content: '⚠ '; }
.actions { white-space: nowrap; display: flex; gap: var(--s2); }
.add { max-width: 28rem; }
</style>
