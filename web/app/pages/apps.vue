<script setup lang="ts">
import type { AppGrant } from '~/types/api'
import { errorMessage } from '~/utils/api'
import { formatDate, formatRelative } from '~/utils/format'

useHead({ title: 'Apps · starstar' })

const api = useApi()
const { can } = useSession()
const everyone = ref(false)
const grants = ref<AppGrant[] | null>(null)
const error = ref<string | null>(null)
const now = Date.now()

async function load() {
  error.value = null
  try {
    grants.value = (await api<AppGrant[]>('/api/apps', { query: { all: everyone.value ? true : undefined } })) ?? []
  } catch (e) {
    error.value = errorMessage(e)
  }
}
onMounted(load)

function choose(all: boolean) {
  if (everyone.value === all) return
  everyone.value = all
  void load()
}

const revoking = ref<AppGrant | null>(null)
const busy = ref(false)
const revokeError = ref<string | null>(null)

async function revoke() {
  const g = revoking.value
  if (!g) return
  busy.value = true
  revokeError.value = null
  try {
    await api(`/api/apps/${encodeURIComponent(g.id)}`, { method: 'DELETE' })
    grants.value = (grants.value ?? []).filter((x) => x.id !== g.id)
    revoking.value = null
  } catch (e) {
    revokeError.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="stack">
    <div class="titlebar">
      <h1>Connected apps</h1>
      <div v-if="can('admin')" class="seg" role="group" aria-label="Which apps">
        <button type="button" class="btn btn-small" :class="everyone ? 'btn-secondary' : 'btn-primary'" :aria-pressed="!everyone" @click="choose(false)">My apps</button>
        <button type="button" class="btn btn-small" :class="everyone ? 'btn-primary' : 'btn-secondary'" :aria-pressed="everyone" @click="choose(true)">Everyone's apps</button>
      </div>
    </div>

    <Notice v-if="error" kind="error">{{ error }} <button type="button" class="btn btn-small btn-secondary" @click="load">Try again</button></Notice>

    <p v-if="grants && !grants.length" class="tool">No connected apps. When the starstar CLI or an MCP client like Claude Code signs in, it shows up here.</p>

    <div v-else-if="grants" class="tool table-wrap">
      <table>
        <thead>
          <tr>
            <th scope="col">App</th>
            <th v-if="everyone" scope="col">User</th>
            <th scope="col">Access</th><th scope="col">Connected</th><th scope="col">Last used</th><th scope="col">Expires</th>
            <th scope="col"><span class="visually-hidden">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="g in grants" :key="g.id">
            <th scope="row" class="app"><span>{{ g.client_name }}</span><br><span class="small muted">{{ g.client_id }}</span></th>
            <td v-if="everyone">{{ g.username }}</td>
            <td>{{ g.role }}</td>
            <td class="num">{{ formatDate(g.created_at) }}</td>
            <td class="num">{{ g.last_used_at ? formatRelative(g.last_used_at, now) : 'Never' }}</td>
            <td class="num">{{ formatDate(g.expires_at) }}</td>
            <td><button type="button" class="btn btn-small btn-secondary" @click="revokeError = null; revoking = g">Revoke</button></td>
          </tr>
        </tbody>
      </table>
    </div>

    <ConfirmDialog
      :open="!!revoking"
      :title="`Disconnect ${revoking?.client_name ?? ''}?`"
      body="It loses access right away and will have to ask again."
      confirm-label="Disconnect"
      danger
      :busy="busy"
      :error="revokeError"
      @confirm="revoke"
      @cancel="revoking = null"
    />
  </section>
</template>

<style scoped>
.titlebar { display: flex; justify-content: space-between; align-items: center; gap: var(--s4); flex-wrap: wrap; }
.titlebar h1 { margin: 0; }
.seg { display: flex; gap: var(--s2); }
.app { font-weight: 600; overflow-wrap: anywhere; }
.app .small { font-weight: 400; }
</style>
