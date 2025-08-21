const CACHE = "logtailer-cache-v1";
const ASSETS = [
    "/",
    "/index.html",
    "/manifest.webmanifest",
    "/icons/icon-192.png",
    "/icons/icon-512.png"
    // Hinweis: Trunk hashed Wasm/JS-Dateien. Optional dynamisch cachen.
];

self.addEventListener("install", (event) => {
    event.waitUntil(
        caches.open(CACHE).then((cache) => cache.addAll(ASSETS))
    );
});

self.addEventListener("activate", (event) => {
    event.waitUntil(
        caches.keys().then((keys) =>
            Promise.all(keys.filter(k => k !== CACHE).map(k => caches.delete(k)))
        )
    );
});

self.addEventListener("fetch", (event) => {
    const req = event.request;
    // Network-first for navigation; cache-first for assets
    if (req.mode === "navigate") {
        event.respondWith(
            fetch(req).catch(() => caches.match("/index.html"))
        );
    } else {
        event.respondWith(
            caches.match(req).then((hit) => hit || fetch(req))
        );
    }
});