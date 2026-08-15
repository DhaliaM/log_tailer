use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use futures::StreamExt;
use js_sys::Reflect;
use leptos::prelude::{GetUntracked, RwSignal, Set, Update};
use leptos::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use std::collections::{HashSet, HashMap};
use crate::domain::types::{Stats, Event, LootItemConfig, CreatureConfig};
use crate::domain::parser::extract_timestamp_sec;
use crate::persistence::idb::Db;
use crate::services::fs_access::{getFileFromHandle, sliceText};

pub fn start_polling(
    handle: JsValue,
    offset: RwSignal<u64>,
    stats: RwSignal<Stats>,
    db: Db,
    running_flag: Arc<AtomicBool>,
    cancel_token: Arc<AtomicBool>,
    dbg_lines: RwSignal<u64>,
    dbg_events: RwSignal<u64>,
    mu_map: RwSignal<HashMap<String, f64>>,
    creature_sig: RwSignal<String>,
    last_event_ts: RwSignal<u64>,
) {
    spawn_local(async move {
        let mut tick = gloo_timers::future::IntervalStream::new(1000).fuse();

        let mut last_loot_ts: Option<String> = None;
        let mut prev_running = false;
        let mut seen_items: HashSet<String> = HashSet::new();

        // ── Kill-Tracking für Maturity + Kill-Loot ────────────────────────────
        // Damage accumulated since last kill commit (= damage for current kill)
        let mut kill_damage_acc: f64 = 0.0;
        // Pending commit: loot and damage for the kill group we just finished
        let mut pending_loot: f64 = 0.0;
        let mut pending_damage: f64 = 0.0;
        let mut pending_ts: u64 = 0;
        let mut has_pending: bool = false;

        // Creature configs – geladen beim ersten Tick, danach alle 30 Ticks
        let mut creature_configs: Vec<CreatureConfig> = vec![];
        let mut config_refresh_counter: u32 = 0;
        let mut mu_refresh_counter: u32 = 0;


        while tick.next().await.is_some() {
            if cancel_token.load(Ordering::Relaxed) { break; }

            let is_running = running_flag.load(Ordering::Relaxed);

            let file_js = if Reflect::has(&handle, &JsValue::from_str("getFile")).unwrap_or(false) {
                match getFileFromHandle(&handle).await {
                    Ok(f) => f,
                    Err(e) => { log::warn!("getFileFromHandle failed: {:?}", e); continue; }
                }
            } else {
                handle.clone()
            };

            let size = Reflect::get(&file_js, &JsValue::from_str("size"))
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0);

            if !is_running {
                if prev_running {
                    last_loot_ts = None;
                    kill_damage_acc = 0.0;
                    pending_loot = 0.0;
                    pending_damage = 0.0;
                    has_pending = false;
                }
                prev_running = false;
                continue;
            }
            prev_running = true;

            // Creature configs periodisch nachladen
            if config_refresh_counter == 0 {
                if let Ok(cfgs) = db.get_all_creatures().await {
                    creature_configs = cfgs;
                }
            }
            config_refresh_counter = (config_refresh_counter + 1) % 30;

            let mut pos = offset.get_untracked() as f64;
            if size < pos { pos = 0.0; }
            if size == pos {
                // mu_map alle 30s aktualisieren (nicht bei jedem Idle-Tick)
                if mu_refresh_counter == 0 {
                    if let Ok(items) = db.get_all_loot_item_configs().await {
                        let map: HashMap<String, f64> = items.iter().map(|c| (c.name.clone(), c.mu_pct)).collect();
                        mu_map.set(map);
                    }
                }
                mu_refresh_counter = (mu_refresh_counter + 1) % 30;
                continue;
            }
            mu_refresh_counter = 0; // Bei neuen Events sofort aktualisieren

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
                            last_event_ts.set(crate::services::run_service::now_sec());

                            // In-Memory Stats
                            stats.update(|s| s.apply_event(&ev));

                            // ── Damage-Tracking für Maturity ─────────────────
                            if let Event::PlayerHit { damage, .. } = &ev {
                                kill_damage_acc += damage;
                            }

                            // ── Dropper-Update für neue Loot-Items ───────────
                            if let Event::Loot { ref item, .. } = ev {
                                if !seen_items.contains(item.as_str()) {
                                    seen_items.insert(item.clone());
                                    let db2 = db.clone();
                                    let name = item.clone();
                                    let dropper = creature_sig.get_untracked();
                                    spawn_local(async move {
                                        if let Ok(all) = db2.get_all_loot_item_configs().await {
                                            if let Some(existing) = all.into_iter().find(|c| c.name == name) {
                                                if !dropper.is_empty() && !existing.droppers.contains(&dropper) {
                                                    let mut updated = existing.clone();
                                                    updated.droppers.push(dropper);
                                                    updated.last_updated_sec = Some(crate::services::run_service::now_sec());
                                                    let _ = db2.save_loot_item_config(&updated).await;
                                                }
                                            } else {
                                                let droppers = if dropper.is_empty() { vec![] } else { vec![dropper] };
                                                let _ = db2.save_loot_item_config(&LootItemConfig {
                                                    name, mu_pct: 100.0, droppers,
                                                    last_updated_sec: Some(crate::services::run_service::now_sec()),
                                                }).await;
                                            }
                                        }
                                    });
                                }
                            }

                            // ── Kill-Erkennung & Maturity-Tracking ───────────
                            if let Event::Loot { value_ped, .. } = &ev {
                                let ts_str = line.get(0..19).map(|s| s.to_string());
                                let ts_num = extract_timestamp_sec(line).unwrap_or(0);

                                let is_new_kill = match (&last_loot_ts, &ts_str) {
                                    (Some(prev), Some(cur)) if prev == cur => false,
                                    _ => true,
                                };

                                if is_new_kill {
                                    // Pending kill committen (kill N-1)
                                    if has_pending {
                                        let creature = creature_sig.get_untracked();
                                        let cfg = creature_configs.iter().find(|c| c.creature == creature);
                                        let loot  = pending_loot;
                                        let ts    = pending_ts;
                                        let dmg   = pending_damage;
                                        stats.update(|s| s.record_kill(loot, ts, cfg, dmg));
                                    }
                                    // Kill N als pending sichern
                                    pending_loot   = *value_ped;
                                    pending_damage = kill_damage_acc;
                                    pending_ts     = ts_num;
                                    kill_damage_acc = 0.0;
                                    has_pending = true;
                                    last_loot_ts = ts_str;
                                } else {
                                    // Gleiche Kill-Gruppe: Loot akkumulieren
                                    pending_loot += value_ped;
                                }
                            }

                            // ── Delta für IDB ─────────────────────────────────
                            use Event::*;
                            match ev {
                                PlayerHit { damage, critical } => {
                                    delta.total_damage += damage;
                                    if critical { delta.player_crit_hits += 1; }
                                    else         { delta.player_hits += 1; }
                                }
                                EnemyEvaded  => delta.player_evades += 1,
                                EnemyMiss    => delta.enemy_misses += 1,
                                PlayerMiss   => delta.player_misses += 1,
                                PlayerEvaded => {}
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

                    offset.set(size as u64);

                    // mu_map aus DB aktualisieren
                    if let Ok(items) = db.get_all_loot_item_configs().await {
                        let map: HashMap<String, f64> = items.iter().map(|c| (c.name.clone(), c.mu_pct)).collect();
                        mu_map.set(map);
                    }
                }
                Err(e) => { log::warn!("sliceText failed: {:?}", e); }
            }
        }
    });
}
