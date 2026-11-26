<template>
  <div>
    <a class="skip-link" href="#main">Skip to content</a>
    <header class="bar">
      <NuxtLink class="wordmark" to="/">starstar</NuxtLink>
      <nav aria-label="Main">
        <NuxtLink to="/">Printers</NuxtLink>
      </nav>
    </header>
    <main id="main" class="page"><slot /></main>
  </div>
</template>
