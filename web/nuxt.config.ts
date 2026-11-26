// Dev: `npm run dev` proxies /api, /oauth, /mcp and /.well-known to the API
// (STARSTAR_API, default http://127.0.0.1:8080). Run the API with
// STARSTAR_PUBLIC_URL=http://127.0.0.1:3000 so OAuth redirects match the dev origin.
const apiTarget = process.env.STARSTAR_API ?? 'http://127.0.0.1:8080'

export default defineNuxtConfig({
  ssr: false,
  compatibilityDate: '2026-11-26',
  devtools: { enabled: false },
  css: [
    '@fontsource/atkinson-hyperlegible-next/400.css',
    '@fontsource/atkinson-hyperlegible-next/600.css',
    '@fontsource/atkinson-hyperlegible-next/700.css',
    '~/assets/css/tokens.css',
    '~/assets/css/base.css',
  ],
  app: {
    head: {
      htmlAttrs: { lang: 'en' },
      title: 'starstar',
      meta: [
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
        { name: 'color-scheme', content: 'light dark' },
      ],
    },
  },
  nitro: {
    // Only the SPA shell: client routes are served by the API's fallback, not as prerendered directories.
    prerender: { crawlLinks: false, routes: [] },
    devProxy: Object.fromEntries(
      ['/api', '/oauth', '/mcp', '/.well-known'].map((p) => [
        p,
        { target: `${apiTarget}${p}`, changeOrigin: false },
      ]),
    ),
  },
  typescript: { strict: true, typeCheck: false },
})
