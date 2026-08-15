use leptos::prelude::Effect;
use leptos::*;
use crate::components::app_shell::AppShell;

#[component]
pub fn App() -> impl IntoView {
    #[cfg(feature = "csr")]
    Effect::new(|_| crate::pwa::register_service_worker());

    view! {
        <AppShell />
    }
}
