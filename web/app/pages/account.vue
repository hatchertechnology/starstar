<script setup lang="ts">
import type { ChangePassword } from '~/types/api'
import { ApiRequestError, errorMessage } from '~/utils/api'

useHead({ title: 'Account · starstar' })

const api = useApi()
const session = useSession()
const current = ref('')
const next = ref('')
const confirm = ref('')
const pending = ref(false)
const error = ref<string | null>(null)
const mismatch = ref<string | null>(null)
const done = ref(false)

async function submit() {
  error.value = null
  mismatch.value = null
  done.value = false
  if (next.value !== confirm.value) {
    mismatch.value = "The new passwords don't match."
    return
  }
  pending.value = true
  const body: ChangePassword = { current_password: current.value, new_password: next.value }
  try {
    await api('/api/me/password', { method: 'PUT', json: body })
    done.value = true
  } catch (e) {
    error.value = e instanceof ApiRequestError && e.status === 403 ? 'Your current password is incorrect.' : errorMessage(e)
  } finally {
    current.value = ''
    next.value = ''
    confirm.value = ''
    pending.value = false
  }
}

async function signOut() {
  await session.signOut()
  await navigateTo('/login')
}
</script>

<template>
  <section class="stack account">
    <h1>Account</h1>
    <p v-if="session.user.value">Signed in as {{ session.user.value.username }} ({{ session.user.value.role }})</p>

    <form class="tool" @submit.prevent="submit">
      <h2>Change password</h2>
      <Notice v-if="error" kind="error">{{ error }}</Notice>
      <Notice v-if="done">Password changed. Your other sessions and all connected apps were signed out.</Notice>
      <FieldRow id="current-password" label="Current password">
        <template #default="{ describedby }">
          <input id="current-password" v-model="current" type="password" autocomplete="current-password" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="new-password" label="New password">
        <template #default="{ describedby }">
          <input id="new-password" v-model="next" type="password" autocomplete="new-password" required :aria-describedby="describedby">
        </template>
      </FieldRow>
      <FieldRow id="confirm-password" label="Confirm new password" :error="mismatch">
        <template #default="{ describedby, invalid }">
          <input id="confirm-password" v-model="confirm" type="password" autocomplete="new-password" required :aria-describedby="describedby" :aria-invalid="invalid">
        </template>
      </FieldRow>
      <button class="btn btn-primary" type="submit" :disabled="pending" :aria-busy="pending">{{ pending ? 'Changing…' : 'Change password' }}</button>
    </form>

    <div>
      <button type="button" class="btn btn-secondary" @click="signOut">Sign out</button>
    </div>
  </section>
</template>

<style scoped>
.account { max-width: 30rem; }
</style>
