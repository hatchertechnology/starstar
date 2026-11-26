<script setup lang="ts">
import type { ConsentRequest } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'
import { formatDate } from '~/utils/format'

definePageMeta({ layout: 'bare' })
useHead({ title: 'Allow access · starstar' })

const route = useRoute()
const api = useApi()
const session = useSession()

const id = computed(() => (typeof route.query.request === 'string' ? route.query.request : ''))
const errorCode = computed(() => (typeof route.query.error === 'string' ? route.query.error : ''))
const request = ref<ConsentRequest | null>(null)
const gone = ref(false)
const failure = ref<string | null>(null)
const loading = ref(true)

const problems: Record<string, string> = {
  expired: 'This request expired. Start the sign-in again from your app.',
  csrf: 'Your session check failed. Reload this page and try again.',
  role: "That access level isn't allowed for this request. Choose again.",
}
const problem = computed(() => (errorCode.value ? (problems[errorCode.value] ?? 'Something went wrong with that decision. Try again.') : null))

onMounted(async () => {
  if (!id.value) {
    gone.value = true
    loading.value = false
    return
  }
  try {
    request.value = await api<ConsentRequest>(`/oauth/authorize/requests/${encodeURIComponent(id.value)}`)
    gone.value = !request.value
  } catch (e) {
    if (e instanceof ApiRequestError && e.status === 404) gone.value = true
    else failure.value = errorMessage(e)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <section class="tool">
    <p v-if="loading" class="muted">Loading…</p>

    <template v-else-if="gone">
      <h1>This sign-in request has expired or was already used</h1>
      <Notice v-if="problem" kind="error">{{ problem }}</Notice>
      <p>Go back to the app or terminal that sent you here and start again.</p>
    </template>

    <template v-else-if="failure">
      <h1>Couldn't load this request</h1>
      <Notice kind="error">{{ failure }}</Notice>
    </template>

    <template v-else-if="request && session.user.value">
      <h1>{{ request.client_name }} wants to use your printers</h1>
      <Notice v-if="problem" kind="error">{{ problem }}</Notice>
      <p v-if="request.client_name_self_declared">
        <span class="chip">self-declared name</span>
        <span class="muted small"> Anyone can register an app with any name. Only continue if you just started this sign-in yourself.</span>
      </p>
      <dl class="facts">
        <dt>Returns you to</dt><dd class="num">{{ request.redirect_host }}</dd>
        <dt>App registered</dt><dd>{{ formatDate(request.client_registered_at) }}</dd>
        <dt>Asked for</dt><dd>{{ request.requested_role }}</dd>
        <dt>Signed in as</dt><dd>{{ session.user.value.username }} ({{ request.user_role }})</dd>
      </dl>
      <hr class="hairline">
      <ConsentForm :request="request" :csrf="session.user.value.csrf_token" />
    </template>
  </section>
</template>

<style scoped>
.facts { display: grid; grid-template-columns: max-content 1fr; gap: var(--s1) var(--s4); margin: 0 0 var(--s3); }
.facts dt { color: var(--ink-soft); }
.facts dd { margin: 0; overflow-wrap: anywhere; }
</style>
