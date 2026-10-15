// Cache only the app shell. Never cache credentials, API responses or conversations.
const CACHE='jarvis-shell-v1';
const SHELL=['/','/app.js','/app.css','/manifest.webmanifest','/icon.svg'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(SHELL))));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE).map(k=>caches.delete(k))))));
self.addEventListener('fetch',event=>{const url=new URL(event.request.url);if(event.request.method==='GET'&&url.origin===self.location.origin&&SHELL.includes(url.pathname)){event.respondWith(fetch(event.request).catch(()=>caches.match(event.request)));}});
