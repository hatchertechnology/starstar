<script setup lang="ts">
import type { PrinterSummary } from '~/types/api'
import { errorMessage } from '~/utils/api'

useHead({ title: 'Printers · starstar' })

const api = useApi()
const { can } = useSession()
const printers = ref<PrinterSummary[] | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)
const states = reactive<Record<string, string>>({})

async function load() {
  loading.value = true
  error.value = null
  try {
    printers.value = (await api<PrinterSummary[]>('/api/printers')) ?? []
    for (const k of Object.keys(states)) if (!printers.value.some((p) => p.name === k)) delete states[k]
  } catch (e) {
    error.value = errorMessage(e)
  } finally {
    loading.value = false
  }
}
onMounted(load)

function onState(name: string, tone: string) { states[name] = tone }

const summary = computed(() => {
  const counts = { active: 0, rest: 0, attention: 0, offline: 0 }
  for (const t of Object.values(states)) {
    if (t === 'active') counts.active++
    else if (t === 'rest') counts.rest++
    else if (t === 'offline') counts.offline++
    else counts.attention++
  }
  const parts = [
    counts.active && `${counts.active} printing`,
    counts.rest && `${counts.rest} idle`,
    counts.attention && `${counts.attention} ${counts.attention === 1 ? 'needs' : 'need'} attention`,
    counts.offline && `${counts.offline} not answering`,
  ].filter(Boolean)
  return parts.join(' · ')
})
</script>

<template>
  <section class="board-ground board-section">
    <div class="titlebar">
      <div>
        <h1>Printers</h1>
        <p class="summary muted num" aria-live="polite">{{ summary }}</p>
      </div>
      <button type="button" class="btn btn-secondary" :disabled="loading" @click="load">Refresh list</button>
    </div>

    <Notice v-if="error" kind="error">
      {{ error }}
      <button type="button" class="btn btn-small btn-secondary" @click="load">Try again</button>
    </Notice>

    <ul v-if="printers && printers.length" class="board">
      <li v-for="p in printers" :key="p.name"><PrinterCard :printer="p" @state="onState" /></li>
    </ul>
    <ul v-else-if="printers" class="board">
      <li>
        <article class="tool empty is-shadow-empty">
          <PrinterSilhouette kind="unknown" variant="shadow" />
          <h2>No printers yet</h2>
          <p v-if="can('admin')">Add one from a terminal: <code>starstar printer add &lt;name&gt; --kind mk4 --host &lt;address&gt; --username &lt;printer user&gt;</code></p>
          <p v-else>Ask an admin to add one.</p>
        </article>
      </li>
    </ul>
    <ul v-else-if="!error" class="board" aria-busy="true">
      <li v-for="n in 3" :key="n"><div class="tool skeleton-card" /></li>
    </ul>
  </section>
</template>

<style scoped>
.board-section { margin: calc(var(--s6) * -1); padding: var(--s6); min-height: calc(100vh - 4rem); }
.titlebar { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--s4); margin-bottom: var(--s4); }
.titlebar h1 { margin-bottom: var(--s1); }
.summary { margin: 0; min-height: 1.4em; }
.empty { background: transparent; border-style: dashed; min-height: 15rem; display: flex; flex-direction: column; gap: var(--s2); }
.empty h2 { margin: 0; font-size: var(--step-1); }
.empty p { margin: 0; overflow-wrap: anywhere; }
.skeleton-card { height: 15rem; background: transparent; border-style: dashed; }
code { font-family: ui-monospace, monospace; font-size: var(--step--1); }
@media (max-width: 40rem) { .board-section { margin: calc(var(--s4) * -1); padding: var(--s4); } }
</style>
