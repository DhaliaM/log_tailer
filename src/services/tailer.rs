use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use futures::StreamExt;
use js_sys::Reflect;
use leptos::prelude::{GetUntracked, RwSignal, Set, Update};
use leptos::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use crate::domain::types::{Stats, Event};
use crate::persistence::idb::Db;
use crate::services::fs_access::{getFileFromHandle, sliceText};

pub fn start_polling(
    handle: JsValue,
    offset: RwSignal<f64>,
    stats: RwSignal<Stats>,
    db: Db,
    running_flag: Arc<AtomicBool>, // statt ReadSignal<bool>
    cancel_token: Arc<AtomicBool>,
    dbg_lines: RwSignal<u64>,
    dbg_events: RwSignal<u64>,
) {
    spawn_local(async move {
        let mut tick = gloo_timers::future::IntervalStream::new(1000).fuse();

        let mut last_loot_ts: Option<String> = None;
        let mut prev_running = false;

        while tick.next().await.is_some() {
            // Abbruch, wenn Komponente disposed wurde
            if cancel_token.load(Ordering::Relaxed) {
                break;
            }

            let is_running = running_flag.load(Ordering::Relaxed);

            // Handle → File/Blob bestimmen
            let file_js = if Reflect::has(&handle, &JsValue::from_str("getFile")).unwrap_or(false) {
                match getFileFromHandle(&handle).await {
                    Ok(f) => f,
                    Err(e) => { log::warn!("getFileFromHandle failed: {:?}", e); continue; }
                }
            } else {
                handle.clone()
            };

            // Dateigröße
            let size = Reflect::get(&file_js, &JsValue::from_str("size"))
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0);

            // Stop-Übergang: optional vorspulen
            if !is_running {
                if prev_running {
                    // Run wurde soeben beendet → nächste Kill-Gruppierung neu beginnen
                    last_loot_ts = None;
                }
                prev_running = false;
                continue;
            }
            prev_running = true;

            let mut pos = offset.get_untracked();
            if size < pos { pos = 0.0; }
            if size == pos { continue; }

            match sliceText(&file_js, pos, size).await {
                Ok(txt_js) => {
                    let chunk = txt_js.as_string().unwrap_or_default();
                    let mut delta = Stats::default();
                    let mut lines_total:  u64 = 0;
                    let mut events_match: u64 = 0;

                    for line in chunk.lines() {
                        lines_total += 1;

                        if let Some(ev) = crate::domain::parser::parse_line(line) {
                            events_match += 1;

                            // In-Memory
                            stats.update(|s| s.apply_event(&ev));

                            if let Loot { .. } = ev {
                                let ts = line.get(0..19).map(|s| s.to_string());
                                let is_new_kill = match (&last_loot_ts, &ts) {
                                    (Some(prev), Some(cur)) if prev == cur => false,
                                    _ => true,
                                };
                                if is_new_kill { stats.update(|s| s.record_kill()); }
                                last_loot_ts = ts;
                            }

                            use Event::*;
                            match ev {
                                PlayerHit { damage, critical } => {
                                    delta.total_damage += damage;
                                    if critical { delta.player_crit_hits += 1; }
                                    else         { delta.player_hits += 1; }
                                }
                                EnemyEvaded                       => delta.player_evades += 1,
                                EnemyMiss                         => delta.enemy_misses += 1,
                                PlayerMiss                        => delta.player_misses += 1,
                                PlayerEvaded                      => delta.player_evades += 1,
                                _ => {}
                            }
                        }
                    }

                    if lines_total > 0 {
                        dbg_lines.update(|x| *x += lines_total);
                        dbg_events.update(|x| *x += events_match);
                    }

                    if delta.player_attacks() > 0 || delta.enemy_misses > 0 {
                        if let Err(e) = db.bump_stats(delta).await {
                            log::warn!("IDB bump_stats: {:?}", e);
                        }
                    }

                    offset.set(size);
                }
                Err(e) => { log::warn!("sliceText failed: {:?}", e); }
            }
        }
    });
}

