#[cfg(feature = "csr")]
pub fn register_service_worker() {
    use web_sys::window;

    if let Some(win) = window() {
        let sw = win.navigator().service_worker(); // kein Option
        // wir ignorieren das Result absichtlich (Browser regelt Registrierung)
        let _ = sw.register("/service-worker.js");
    }
}