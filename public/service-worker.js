const CACHE = "logtailer-cache-v2"; // erhöhe bei neuen Deploys
const SHELL = [
    "/",                     // Start
    "/index.html",
    "/manifest.webmanifest",
    "/icons/icon-144.png",
    "/icons/icon-512.png"
];

// beim Install: App-Shell precachen
self.addEventListener("install", (event) => {
    event.waitUntil(
        caches.open(CACHE)
            .then((cache) => cache.addAll(SHELL))
            .then(() => self.skipWaiting()) // sofort aktivierbar machen
    );
});

// alte Caches weg + Kontrolle übernehmen
self.addEventListener("activate", (event) => {
    event.waitUntil(
        caches.keys().then((keys) =>
            Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
        ).then(() => self.clients.claim())
    );
});

self.addEventListener("message", (event) => {
    const msg = event.data;
    if (msg && msg.type === "CACHE_URLS" && Array.isArray(msg.urls)) {
        event.waitUntil(
            caches.open(CACHE).then(cache => cache.addAll(msg.urls))
        );
    }
});

// Fetch-Strategien
self.addEventListener("fetch", (event) => {
    const req = event.request;
    const url = new URL(req.url);

    // 1) SPA-Navigation: offline auf index.html fallen
    if (req.mode === "navigate") {
        event.respondWith(
            caches.match("/index.html").then(r => r || fetch(req))
        );
        return;
    }

    // Nur gleiche Origin cachen
    if (url.origin === location.origin) {
        const pathname = url.pathname;

        // 2) Statische Assets: Cache-first + dynamisch nachladen & speichern
        const isStatic =
            pathname.endsWith(".wasm") ||
            pathname.endsWith(".js")   ||
            pathname.endsWith(".css")  ||
            pathname.endsWith(".png")  ||
            pathname.endsWith(".jpg")  ||
            pathname.endsWith(".jpeg") ||
            pathname.endsWith(".svg")  ||
            pathname.startsWith("/pkg/") || // Trunk/WASM landen oft unter /pkg/
            pathname.startsWith("/assets/");

        if (isStatic) {
            event.respondWith(
                caches.match(req).then((hit) => {
                    if (hit) return hit; // aus Cache
                    return fetch(req).then((res) => {
                        // bei Erfolg ins Cache legen (Headers bleiben erhalten, inkl. application/wasm)
                        const clone = res.clone();
                        caches.open(CACHE).then((c) => c.put(req, clone)).catch(() => {});
                        return res;
                    }).catch(() => caches.match(req)); // offline: evtl. ältere Kopie
                })
            );
            return;
        }
    }

    // 3) Default: Netz versuchen, offline ggf. aus Cache
    event.respondWith(
        fetch(req).catch(() => caches.match(req))
    );
});
