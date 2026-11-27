<script setup lang="ts">
const session = useSession()
const { user, can } = session

async function signOut() {
  await session.signOut()
  await navigateTo('/login')
}
</script>

<template>
  <div>
    <a class="skip-link" href="#main">Skip to content</a>
    <header class="bar">
      <NuxtLink class="wordmark" to="/">starstar</NuxtLink>
      <nav aria-label="Main">
        <NuxtLink to="/">Printers</NuxtLink>
        <NuxtLink to="/library">Library</NuxtLink>
        <NuxtLink to="/apps">Apps</NuxtLink>
        <NuxtLink v-if="can('admin')" to="/users">Users</NuxtLink>
        <NuxtLink to="/account">Account</NuxtLink>
      </nav>
      <div v-if="user" class="who">
        <span class="username">{{ user.username }}</span>
        <span class="chip role-chip">{{ user.role }}</span>
        <button type="button" class="btn btn-small" @click="signOut">Sign out</button>
      </div>
    </header>
    <main id="main" class="page"><slot /></main>
  </div>
</template>
