use leptos::prelude::ElementChild;
use leptos::*;
use leptos::prelude::{ClassAttribute, Effect};
use crate::components::log_tailer::LogTailer;
#[component]
pub fn App() -> impl IntoView {
    #[cfg(feature = "csr")]
    Effect::new(|_| crate::pwa::register_service_worker());

    view! {
      <main class="p-4 font-sans">
        <h1 class="text-xl">"Log Tailer PWA"</h1>
        <p class="text-sm opacity-80">"Datei wählen, tailen, parsen, in IndexedDB zählen."</p>
        <LogTailer />
      </main>
    }
}