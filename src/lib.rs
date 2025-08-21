use leptos::*;
use leptos::prelude::*;
use wasm_bindgen::prelude::wasm_bindgen;

mod app;
mod components;
mod services;
mod domain;
mod analytics;
mod persistence;
mod pwa;

#[wasm_bindgen(start)]
pub fn wasm_start() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();
    #[cfg(debug_assertions)]
    wasm_logger::init(wasm_logger::Config::default());
    mount_to_body(app::App);
}