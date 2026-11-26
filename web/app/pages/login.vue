<script setup lang="ts">
import { ApiRequestError, errorMessage } from '~/utils/api'
import { safeNext } from '~/utils/next'

definePageMeta({ layout: 'bare', public: true })
useHead({ title: 'Sign in · starstar' })

const route = useRoute()
const session = useSession()
const username = ref('')
const password = ref('')
const pending = ref(false)
const error = ref<string | null>(null)

async function submit() {
  pending.value = true
  error.value = null
  try {
    await session.signIn(username.value, password.value)
    await navigateTo(safeNext(route.query.next), { replace: true })
  } catch (e) {
    password.value = ''
    if (e instanceof ApiRequestError && e.status === 401) error.value = 'Username or password is incorrect.'
    else if (e instanceof ApiRequestError && e.status === 429) error.value = 'Too many attempts. Wait 15 minutes and try again.'
    else error.value = errorMessage(e)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <section class="tool">
    <h1>Sign in to starstar</h1>
    <Notice v-if="error" kind="error">{{ error }}</Notice>
    <form @submit.prevent="submit">
      <FieldRow id="username" label="Username">
        <template #default="{ describedby }">
          <input id="username" v-model="username" name="username" autocomplete="username" autocapitalize="none" spellcheck="false" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="password" label="Password">
        <template #default="{ describedby }">
          <input id="password" v-model="password" name="password" type="password" autocomplete="current-password" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <button class="btn btn-primary" type="submit" :disabled="pending" :aria-busy="pending">{{ pending ? 'Signing in…' : 'Sign in' }}</button>
    </form>
    <p class="link-row">Setting up a new starstar? <NuxtLink to="/setup">Use the setup code.</NuxtLink></p>
  </section>
</template>
