<script setup lang="ts">
import { ApiRequestError, errorMessage } from '~/utils/api'

definePageMeta({ layout: 'bare', public: true })
useHead({ title: 'Set up · starstar' })

const session = useSession()
const flash = useFlash()
const code = ref('')
const username = ref('')
const password = ref('')
const confirm = ref('')
const pending = ref(false)
const error = ref<string | null>(null)
const passwordError = ref<string | null>(null)

async function submit() {
  error.value = null
  passwordError.value = null
  if (password.value.length < 8) {
    passwordError.value = 'Use at least 8 characters.'
    return
  }
  if (password.value !== confirm.value) {
    passwordError.value = "The passwords don't match."
    return
  }
  pending.value = true
  try {
    await session.setup(code.value, username.value, password.value)
    await navigateTo('/', { replace: true })
  } catch (e) {
    password.value = ''
    confirm.value = ''
    if (e instanceof ApiRequestError && e.status === 403) error.value = "That setup code isn't right."
    else if (e instanceof ApiRequestError && e.status === 409) {
      flash.show('starstar is already set up. Sign in.')
      await navigateTo('/login')
    } else if (e instanceof ApiRequestError && e.status === 429) error.value = 'Too many attempts. Wait 15 minutes and try again.'
    else error.value = errorMessage(e)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <section class="tool">
    <h1>Set up starstar</h1>
    <p>Enter the setup code from the starstar API's startup output (or the value of STARSTAR_SETUP_TOKEN). This account becomes the first admin.</p>
    <Notice v-if="error" kind="error">{{ error }}</Notice>
    <form @submit.prevent="submit">
      <FieldRow id="setup-code" label="Setup code">
        <template #default="{ describedby }">
          <input id="setup-code" v-model="code" name="setup_code" autocomplete="off" autocapitalize="none" spellcheck="false" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="username" label="Username" hint="Lowercase letters, digits, dot, dash and underscore.">
        <template #default="{ describedby }">
          <input id="username" v-model="username" name="username" autocomplete="username" autocapitalize="none" spellcheck="false" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="password" label="Password" :error="passwordError">
        <template #default="{ describedby, invalid }">
          <input id="password" v-model="password" name="password" type="password" autocomplete="new-password" required :aria-describedby="describedby" :aria-invalid="invalid">
        </template>
      </FieldRow>
      <FieldRow id="confirm" label="Confirm password">
        <template #default="{ describedby }">
          <input id="confirm" v-model="confirm" name="confirm" type="password" autocomplete="new-password" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <button class="btn btn-primary" type="submit" :disabled="pending" :aria-busy="pending">{{ pending ? 'Creating…' : 'Create admin account' }}</button>
    </form>
    <p class="link-row"><NuxtLink to="/login">Already set up? Sign in.</NuxtLink></p>
  </section>
</template>
