// Kill switch for the old collie service worker living at this origin.
// Installs over it, wipes every cache, and unregisters itself. It has no
// fetch handler, so open clients keep hitting the network directly and shed
// worker control on their next natural navigation — no forced reload.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(keys.map((k) => caches.delete(k)));
      await self.registration.unregister();
    })(),
  );
});
