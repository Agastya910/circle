// Circle service worker. Cache-first for app shell, network-only for /api,
// and a push handler that wakes the app and shows a generic notification.

const SHELL_CACHE = 'circle-shell-v1';
const SHELL = ['/', '/index.html', '/manifest.json', '/style.css'];

self.addEventListener('install', (e) => {
  e.waitUntil(
    caches.open(SHELL_CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== SHELL_CACHE).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (url.pathname.startsWith('/api/')) return; // network-only
  if (e.request.method !== 'GET') return;

  e.respondWith(
    caches.match(e.request).then((cached) =>
      cached ||
      fetch(e.request).then((res) => {
        // Cache WASM and JS chunks after first load.
        if (res.ok && (url.pathname.endsWith('.wasm') || url.pathname.endsWith('.js'))) {
          const clone = res.clone();
          caches.open(SHELL_CACHE).then((c) => c.put(e.request, clone));
        }
        return res;
      }).catch(() => caches.match('/'))
    )
  );
});

self.addEventListener('push', (e) => {
  // v1 sends tickle pushes with no payload. Show a generic "new post" toast;
  // the app fetches the actual content when opened.
  const title = 'Circle';
  const body = 'Someone in your circle posted.';
  e.waitUntil(
    self.registration.showNotification(title, {
      body,
      icon: '/icon-192.png',
      badge: '/icon-192.png',
      tag: 'circle-post',
    })
  );
});

self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  e.waitUntil(
    self.clients.matchAll({ type: 'window' }).then((list) => {
      for (const c of list) {
        if (c.url.includes(self.registration.scope) && 'focus' in c) return c.focus();
      }
      return self.clients.openWindow('/');
    })
  );
});
