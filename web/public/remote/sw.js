// Prism's remote: a service worker so that the phone can install the page as an app, and be
// woken by the Mac. It caches nothing: an answer of the Mac (tickets, MRs, a terminal) never stays
// on the phone. A push carries nothing either: woken, it asks the Mac, over the tunnel and with
// the page's session, which agent waits and in which profile (`/api/wake`); without a session, it
// says only that an agent waits. Or what Prism tells (`text`: a chain's "Notify me"). A tap opens
// Prism on that agent, in its profile.
self.addEventListener('install', () => self.skipWaiting())
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()))
self.addEventListener('fetch', () => {})

self.addEventListener('push', (e) => {
  e.waitUntil(
    (async () => {
      let w = {}
      try {
        const r = await fetch('/api/wake', { credentials: 'same-origin', cache: 'no-store' })
        if (r.ok) w = await r.json()
      } catch {
        // The Mac out of reach a moment: the plain text.
      }
      // Something Prism tells (a chain's "Notify me" step), or an agent waiting.
      const told = typeof w.text === 'string' && w.text
      const title = told ? (w.name ? `${w.name} · Prism` : 'Prism') : w.title ? (w.name ? `${w.name} · ${w.title}` : w.title) : 'Prism'
      const body = told ? w.text : w.title ? 'Waiting for you.' : 'An agent is waiting for you.'
      await self.registration.showNotification(title, { body, tag: told ? 'prism-notice' : 'prism-agent', renotify: true, icon: '/remote/icon-192.png', badge: '/remote/icon-192.png', data: { profile: w.profile ?? null, agent: w.agent ?? null } })
    })(),
  )
})

self.addEventListener('notificationclick', (e) => {
  e.notification.close()
  const { profile, agent } = e.notification.data ?? {}
  e.waitUntil(
    (async () => {
      const open = await self.clients.matchAll({ type: 'window', includeUncontrolled: true })
      const page = open.find((c) => new URL(c.url).origin === self.location.origin)
      if (page) {
        await page.focus()
        page.postMessage(agent ? { t: 'open', profile, agent } : { t: 'view', view: 'agents' })
      } else {
        const q = new URLSearchParams({ view: 'agents' })
        if (profile) q.set('profile', profile)
        if (agent) q.set('agent', agent)
        await self.clients.openWindow(`/?${q}`)
      }
    })(),
  )
})
