use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use futures::StreamExt;
use js_sys::Reflect;
use leptos::prelude::{GetUntracked, RwSignal, Set, Update};
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

use crate::domain::parser::parse_line;
use crate::domain::types::{CraftAttempt, CraftRunStats, Event, is_residue};
use crate::services::fs_access::{getFileFromHandle, sliceText};

/// Startet den Craft-Run-Polling-Loop (1 s Intervall).
pub fn start_craft_polling(
    handle:       JsValue,
    offset:       RwSignal<u64>,
    stats:        RwSignal<CraftRunStats>,
    attempts:     RwSignal<Vec<CraftAttempt>>,
    cancel_token: Arc<AtomicBool>,
) {
    spawn_local(async move {
        let mut tick = gloo_timers::future::IntervalStream::new(1000).fuse();

        let mut batch_ts:    u64 = 0;
        let mut batch_items: Vec<(String, u64, f64)> = vec![];

        while tick.next().await.is_some() {
            if cancel_token.load(Ordering::Relaxed) {
                if !batch_items.is_empty() {
                    let attempt = build_attempt(batch_ts, batch_items);
                    commit(&stats, &attempts, attempt);
                }
                break;
            }

            // Datei-Blob holen
            let file_js = if Reflect::has(&handle, &JsValue::from_str("getFile")).unwrap_or(false) {
                match getFileFromHandle(&handle).await { Ok(f) => f, Err(_) => continue }
            } else {
                handle.clone()
            };

            let file_blob: web_sys::Blob = match file_js.dyn_into() {
                Ok(b) => b, Err(_) => continue,
            };
            let size = file_blob.size() as u64;
            let cur_offset = offset.get_untracked();
            if size <= cur_offset { continue; }

            let text = match sliceText(&file_blob, cur_offset as f64, size as f64).await {
                Ok(js) => js.as_string().unwrap_or_default(),
                Err(_) => continue,
            };
            offset.set(size);

            for line in text.lines() {
                let Some(ev) = parse_line(line) else { continue };
                let Event::Loot { item, qty, value_ped, timestamp_sec } = ev else { continue };

                if batch_items.is_empty() {
                    batch_ts = timestamp_sec;
                    batch_items.push((item, qty, value_ped));
                } else if timestamp_sec <= batch_ts + 1 {
                    batch_ts = timestamp_sec;
                    batch_items.push((item, qty, value_ped));
                } else {
                    let attempt = build_attempt(batch_ts, std::mem::take(&mut batch_items));
                    commit(&stats, &attempts, attempt);
                    batch_ts = timestamp_sec;
                    batch_items.push((item, qty, value_ped));
                }
            }
        }
    });
}

fn build_attempt(timestamp_sec: u64, items: Vec<(String, u64, f64)>) -> CraftAttempt {
    let mut output_items = vec![];
    let mut residue_ped  = 0.0f64;
    let mut shrapnel_ped = 0.0f64;
    let mut total_value  = 0.0f64;

    for (name, qty, ped) in items {
        total_value += ped;
        if name == "Shrapnel" {
            shrapnel_ped += ped;
        } else if is_residue(&name) {
            residue_ped += ped;
        } else {
            output_items.push((name, qty, ped));
        }
    }

    CraftAttempt {
        timestamp_sec, success: !output_items.is_empty(),
        output_items, residue_ped, shrapnel_ped, total_value_ped: total_value,
    }
}

fn commit(stats: &RwSignal<CraftRunStats>, attempts: &RwSignal<Vec<CraftAttempt>>, attempt: CraftAttempt) {
    stats.update(|s| s.apply(&attempt));
    attempts.update(|v| v.push(attempt));
}
