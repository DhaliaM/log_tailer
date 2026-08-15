use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::*;
use js_sys::Reflect;
use wasm_bindgen::JsValue;

use crate::components::app_shell::RunTabActive;
use crate::domain::types::{CreatureConfig, HuntPlan, LootItemConfig, LootItemAgg, Loadout, Weapon, Amplifier, RunConfig, SavedRun, Stats};
use crate::persistence::idb::Db;
use crate::services::fs_access::{self, getFileFromHandle};
use crate::services::run_service::{RunService, RunState};
use crate::services::tailer;

// ─── RunPanel ────────────────────────────────────────────────────────────────

#[component]
pub fn RunPanel() -> impl IntoView {
    // Handle (File/Blob, nicht-Send)
    let (handle, set_handle) = signal_local::<Option<JsValue>>(None);
    let (db, set_db) = signal_local::<Option<Db>>(None);

    // RunService
    let svc = RunService::new();
    let svc = StoredValue::new_local(svc);

    // Polling-Steuerung
    let polling_started = RwSignal::new(false);
    let current_creature = RwSignal::new(String::new());
    let running_flag  = Arc::new(AtomicBool::new(false));
    let cancel_token  = Arc::new(AtomicBool::new(false));
    let dbg_lines  = RwSignal::new(0_u64);
    let dbg_events = RwSignal::new(0_u64);

    let ui_msg = RwSignal::new(String::new());
    let (hunt_plan, set_hunt_plan) = signal_local::<Option<HuntPlan>>(None);
    let (hist_runs, set_hist_runs) = signal_local::<Vec<SavedRun>>(vec![]);
    let enhancer_cost_input = RwSignal::new(String::new());

    // Letzter Event-Zeitpunkt – für Auto-Pause
    let last_event_ts = RwSignal::new(crate::services::run_service::now_sec());

    // Sekunden-Ticker für Laufzeit-Anzeige + Auto-Pause
    let now_sig = RwSignal::new(crate::services::run_service::now_sec());
    {
        use futures::StreamExt;
        let ct = cancel_token.clone();
        leptos::task::spawn_local(async move {
            let mut tick = gloo_timers::future::IntervalStream::new(1000).fuse();
            while tick.next().await.is_some() {
                if ct.load(Ordering::Relaxed) { break; }
                let now = crate::services::run_service::now_sec();
                now_sig.set(now);

                // Auto-Pause nach 5 Minuten ohne Event
                if matches!(svc.get_value().state.get_untracked(), RunState::Running { .. }) {
                    let idle = now.saturating_sub(last_event_ts.get_untracked());
                    if idle >= 300 {
                        svc.get_value().pause();
                    }
                }
            }
        });
    }

    // Laufendes Signal → Flag spiegeln
    {
        let rf = running_flag.clone();
        let svc = svc;
        Effect::new(move |_| {
            let is_running = matches!(svc.get_value().state.get(), RunState::Running { .. });
            rf.store(is_running, Ordering::Relaxed);
        });
    }

    on_cleanup({
        let ct = cancel_token.clone();
        move || { ct.store(true, Ordering::Relaxed); }
    });

    // ── DB öffnen ────────────────────────────────────────────────────────────
    Effect::new({
        let set_db = set_db.clone();
        move |_| {
            spawn_local(async move {
                match Db::open().await {
                    Ok(d) => {
                        // Hunt-Plan beim Start laden
                        if let Ok(p) = d.get_active_hunt_plan().await {
                            set_hunt_plan.set(p);
                        }
                        // Historische Runs für Ausreißer-Warnung
                        let d_hist = d.clone();
                        spawn_local(async move {
                            if let Ok(v) = d_hist.get_all_runs().await {
                                set_hist_runs.set(v);
                            }
                        });
                        set_db.set(Some(d));
                    }
                    Err(e) => log::warn!("DB open: {:?}", e),
                }
            });
        }
    });

    // ── Polling starten wenn Handle + DB verfügbar ────────────────────────
    Effect::new({
        let handle = handle.clone();
        let db = db.clone();
        let svc = svc;
        let running_flag = running_flag.clone();
        let cancel_token = cancel_token.clone();
        move |_| {
            if polling_started.get() { return; }
            let h_opt = handle.get();
            let d_opt = db.get();
            if let (Some(h), Some(d)) = (h_opt, d_opt) {
                polling_started.set(true);
                tailer::start_polling(
                    h.clone(),
                    svc.get_value().offset,
                    svc.get_value().stats,
                    d.clone(),
                    running_flag.clone(),
                    cancel_token.clone(),
                    dbg_lines,
                    dbg_events,
                    svc.get_value().mu_map,
                    current_creature,
                    last_event_ts,
                );
            }
        }
    });

    // ── Datei wählen ─────────────────────────────────────────────────────────
    let on_pick = {
        let set_handle = set_handle.clone();
        let ui_msg = ui_msg.clone();
        move |_| {
            spawn_local(async move {
                let supports_picker = fs_access::hasOpenFilePicker().unwrap_or(false);
                let picked = if supports_picker {
                    match fs_access::pickFile().await {
                        Ok(h) => { ui_msg.set("Datei gewählt (moderner Picker)".into()); Some(h) }
                        Err(_) => None,
                    }
                } else {
                    match fs_access::simpleInputPick().await {
                        Ok(f) => { ui_msg.set("Datei gewählt (Fallback)".into()); Some(f) }
                        Err(_) => None,
                    }
                };
                if let Some(h) = picked {
                    if supports_picker {
                        let _ = fs_access::ensureReadPermission(&h).await;
                    }
                    set_handle.set(Some(h));
                }
            });
        }
    };

    // ── Run konfigurieren ────────────────────────────────────────────────────
    let event_active_sig = RwSignal::new(false);
    let creature_input = RwSignal::new(String::new());
    let loadout_id_input = RwSignal::new(String::new());
    let loadout_name_input = RwSignal::new(String::new());
    let budget_input = RwSignal::new(String::new());
    let budget_warn_input = RwSignal::new(String::from("80"));
    let (loadouts, set_loadouts) = signal_local::<Vec<Loadout>>(vec![]);
    let (creatures, set_creatures) = signal_local::<Vec<CreatureConfig>>(vec![]);
    let (weapons, set_weapons) = signal_local::<Vec<Weapon>>(vec![]);
    let (amps, set_amps) = signal_local::<Vec<Amplifier>>(vec![]);

    // Loadouts + Kreaturen + Weapons + Amps laden — beim DB-Open UND wenn Run-Tab wieder aktiv wird
    let run_tab_active = use_context::<RunTabActive>().map(|c| c.0);
    Effect::new({
        let db = db.clone();
        move |_| {
            // Abhängigkeit auf Tab-Aktivierung registrieren (wenn verfügbar)
            if let Some(sig) = run_tab_active {
                if !sig.get() { return; }
            }
            if let Some(d) = db.get() {
                let d2 = d.clone();
                let d3 = d.clone();
                let d4 = d.clone();
                let d5 = d.clone();
                spawn_local(async move {
                    if let Ok(ls) = d2.get_all_loadouts().await {
                        set_loadouts.set(ls);
                    }
                });
                // Hunt-Plan neu laden (evtl. im Crafting-Tab gesetzt)
                spawn_local(async move {
                    if let Ok(p) = d5.get_active_hunt_plan().await {
                        set_hunt_plan.set(p);
                    }
                });
                spawn_local(async move {
                    if let Ok(cs) = d.get_all_creatures().await {
                        set_creatures.set(cs);
                    }
                });
                spawn_local(async move {
                    if let Ok(ws) = d3.get_all_weapons().await {
                        set_weapons.set(ws);
                    }
                });
                spawn_local(async move {
                    if let Ok(as_) = d4.get_all_amps().await {
                        set_amps.set(as_);
                    }
                });
            }
        }
    });

    let on_configure = {
        let svc = svc;
        let creature_input = creature_input;
        let loadout_id_input = loadout_id_input;
        let budget_input = budget_input;
        let budget_warn_input = budget_warn_input;
        move |_| {
            let creature = creature_input.get_untracked();
            let loadout_id = loadout_id_input.get_untracked();
            let budget_ped = budget_input.get_untracked().parse::<f64>().ok();
            let budget_warn_pct = budget_warn_input.get_untracked().parse::<f64>().ok();
            let enhancer_cost_ped = enhancer_cost_input.get_untracked()
                .parse::<f64>().ok().filter(|&v| v > 0.0);
            let config = RunConfig {
                creature, loadout_id, budget_ped, budget_warn_pct,
                target_hp_note: None, armor_profile_id: None, fap_profile_id: None,
                event_active: event_active_sig.get_untracked(),
                enhancer_cost_ped,
            };
            svc.get_value().configure(config);
        }
    };

    // ── Run starten ──────────────────────────────────────────────────────────
    let on_start = {
        let handle = handle.clone();
        let svc = svc;
        let db = db.clone();
        let loadouts = loadouts.clone();
        let weapons = weapons.clone();
        let amps = amps.clone();
        move |_| {
            let svc = svc;
            let db_opt = db.get_untracked();
            let loadouts_snap = loadouts.get_untracked();
            let weapons_snap = weapons.get_untracked();
            let amps_snap = amps.get_untracked();
            let (config_loadout_id, config_creature) = match svc.get_value().state.get_untracked() {
                RunState::Configured(ref c) => (c.loadout_id.clone(), c.creature.clone()),
                _ => (String::new(), String::new()),
            };
            // Kreatur für Dropper-Tracking im Tailer setzen
            current_creature.set(config_creature);
            spawn_local(async move {
                if let Some(h) = handle.get_untracked() {
                    let size = get_file_size(&h).await;

                    // Loadout → pec_per_shot
                    let pec_per_shot = {
                        let lo = loadouts_snap.iter().find(|l| l.id == config_loadout_id.as_str());
                        if let Some(loadout) = lo {
                            let weapon = weapons_snap.iter().find(|w| w.id == loadout.weapon_id);
                            let amp = loadout.amp_id.as_ref()
                                .and_then(|aid| amps_snap.iter().find(|a| a.id == *aid));
                            if let Some(w) = weapon {
                                loadout.total_pec_per_shot(w, amp)
                            } else { 0.0 }
                        } else { 0.0 }
                    };

                    // mu_map aus DB laden
                    let mu_map = if let Some(ref d) = db_opt {
                        d.get_all_loot_item_configs().await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|c| (c.name, c.mu_pct))
                            .collect::<std::collections::HashMap<String, f64>>()
                    } else {
                        std::collections::HashMap::new()
                    };

                    svc.get_value().set_loadout_context(pec_per_shot, mu_map);
                    svc.get_value().start(size);
                    // Auto-Pause-Timer zurücksetzen: Startzeitpunkt als letzten Event-Timestamp setzen
                    last_event_ts.set(crate::services::run_service::now_sec());
                }
            });
        }
    };

    // ── Pause / Resume ───────────────────────────────────────────────────────
    let on_pause = move |_| { svc.get_value().pause(); };
    let on_resume = move |_| {
        last_event_ts.set(crate::services::run_service::now_sec());
        svc.get_value().resume();
    };

    // ── Skip to now ──────────────────────────────────────────────────────────
    let on_skip = {
        let handle = handle.clone();
        let svc = svc;
        move |_| {
            let h_opt = handle.get_untracked();
            spawn_local(async move {
                if let Some(h) = h_opt {
                    svc.get_value().skip_to_now(&h).await;
                }
            });
        }
    };

    // ── Stop ─────────────────────────────────────────────────────────────────
    let on_stop = move |_| { svc.get_value().stop(); };

    // Notiz-Feld für Stopped-State
    let note_input = RwSignal::new(String::new());

    // ── Speichern ─────────────────────────────────────────────────────────────
    let on_save = {
        let db = db.clone();
        let svc = svc;
        move |_| {
            let db_opt = db.get_untracked();
            let note_val = note_input.get_untracked();
            let note = if note_val.is_empty() { None } else { Some(note_val) };

            spawn_local(async move {
                if let Some(d) = db_opt {
                    let _ = svc.get_value().save(&d, None, None, note).await;
                }
            });
        }
    };

    // ── Verwerfen / Neuer Run ─────────────────────────────────────────────────
    let on_discard = move |_| { svc.get_value().discard(); note_input.set(String::new()); };
    let on_new_run = move |_| { svc.get_value().new_run(); note_input.set(String::new()); };

    // ── View ─────────────────────────────────────────────────────────────────
    view! {
        <div class="space-y-4 max-w-3xl ">
            <h2 class="text-lg font-semibold ">"🎯 Run"</h2>

            // Datei-Status + Debug
            <div style="font-size:12px;color:#64748b;background:#f1f5f9;padding:6px 10px;border-radius:6px;font-family:monospace">
                "Datei: "
                {move || if handle.get().is_some() { "✅" } else { "❌" }}
                " | DB: "
                {move || if db.get().is_some() { "✅" } else { "⏳" }}
                " | Polling: "
                {move || if polling_started.get() { "✅" } else { "❌" }}
                " | Zeilen gelesen: " {move || dbg_lines.get()}
                " | Events: " {move || dbg_events.get()}
            </div>
            {move || if ui_msg.get().is_empty() {
                view!{<span></span>}.into_any()
            } else {
                view!{<div class="text-xs text-blue-600 ">{move || ui_msg.get()}</div>}.into_any()
            }}

            // State-Machine-UI
            {move || {
                let state = svc.get_value().state.get();
                match state {
                    // ── IDLE: Datei wählen ────────────────────────────────────
                    RunState::Idle if handle.get().is_none() => view! {
                        <div class="space-y-2 ">
                            <button class="btn-primary " on:click=on_pick.clone()>
                                "📂 Log-Datei wählen"
                            </button>
                        </div>
                    }.into_any(),

                    // ── IDLE: Datei gewählt → konfigurieren ──────────────────
                    RunState::Idle => view! {
                        <div class="space-y-3 bg-gray-50 p-4 rounded border ">
                            <h3 class="font-medium ">"Run konfigurieren"</h3>
                            <div class="grid gap-2 sm:grid-cols-2 ">
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Kreatur"
                                    <input
                                        class="input "
                                        list="creature-list"
                                        placeholder="z.B. Atrox"
                                        on:input=move |ev| creature_input.set(event_target_value(&ev))
                                    />
                                    <datalist id="creature-list">
                                        {move || creatures.get().iter()
                                            .map(|c| view!{<option value=c.creature.clone() />})
                                            .collect_view()}
                                    </datalist>
                                </label>
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Loadout"
                                    <select
                                        class="input "
                                        on:change=move |ev| {
                                            let val = event_target_value(&ev);
                                            loadout_id_input.set(val.clone());
                                            loadout_name_input.set(val);
                                        }
                                    >
                                        <option value="">"- bitte wählen -"</option>
                                        {move || loadouts.get().iter().map(|l| {
                                            let id = l.id.clone();
                                            let name = l.name.clone();
                                            view!{<option value=id.clone()>{name}</option>}
                                        }).collect_view()}
                                    </select>
                                </label>
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Budget (PED, optional)"
                                    <input
                                        class="input "
                                        type="number" step="0.01"
                                        placeholder="z.B. 50"
                                        on:input=move |ev| budget_input.set(event_target_value(&ev))
                                    />
                                </label>
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Budget-Warnschwelle (%)"
                                    <input
                                        class="input "
                                        type="number" min="50" max="100" step="5"
                                        value="80"
                                        on:input=move |ev| budget_warn_input.set(event_target_value(&ev))
                                    />
                                </label>
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Enhancer-Kosten (PED/Run)"
                                    <input class="input w-24 " type="number" step="0.01" placeholder="0.00"
                                        prop:value=move || enhancer_cost_input.get()
                                        on:input=move |ev| enhancer_cost_input.set(event_target_value(&ev))
                                    />
                                </label>
                            </div>
                            <label class="flex gap-2 items-center text-sm ">
                                <input type="checkbox"
                                    prop:checked=move || event_active_sig.get()
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        let checked = ev.target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            .map(|i| i.checked())
                                            .unwrap_or(false);
                                        event_active_sig.set(checked);
                                    }
                                />
                                "Event aktiv"
                            </label>
                            <button class="btn-primary " on:click=on_configure.clone()>
                                "✅ Konfigurieren (Felder optional)"
                            </button>
                            <button class="btn-secondary ml-2 " on:click=on_pick.clone()>
                                "📂 Andere Datei"
                            </button>
                        </div>
                    }.into_any(),

                    // ── CONFIGURED ───────────────────────────────────────────
                    RunState::Configured(ref config) => view! {
                        <div class="space-y-3 ">
                            <div class="bg-blue-50 p-3 rounded border border-blue-200 text-sm ">
                                <div>"Kreatur: " <strong>{config.creature.clone()}</strong></div>
                                <div>"Loadout-ID: " {config.loadout_id.clone()}</div>
                                {config.budget_ped.map(|b| view!{
                                    <div>"Budget: " {format!("{:.2} PED", b)}</div>
                                })}
                            </div>
                            <div class="space-x-2 ">
                                <button class="btn-primary " on:click=on_start.clone()>
                                    "▶ Run starten"
                                </button>
                                <button class="btn-secondary " on:click=on_discard.clone()>
                                    "✖ Abbrechen"
                                </button>
                            </div>
                        </div>
                    }.into_any(),

                    // ── RUNNING ──────────────────────────────────────────────
                    RunState::Running { ref config, started_at, .. } => {
                        let creature = config.creature.clone();
                        let budget = config.budget_ped;
                        let warn_pct = config.budget_warn_pct.unwrap_or(80.0);
                        let enhancer_cost_run = config.enhancer_cost_ped.unwrap_or(0.0);
                        let creature_for_warn = config.creature.clone();
                        view! {
                            <div class="space-y-3 ">
                                <div class="flex items-center gap-2 ">
                                    <span class="text-green-600 font-semibold ">"● LÄUFT"</span>
                                    <span class="text-sm text-gray-500 ">{creature}</span>
                                    <span class="text-xs font-mono text-gray-400 ">
                                        {move || fmt_elapsed(now_sig.get().saturating_sub(started_at))}
                                    </span>
                                </div>
                                <BudgetBar
                                    stats=svc.get_value().stats.read_only()
                                    pec_per_shot=svc.get_value().pec_per_shot.read_only()
                                    budget=budget warn_pct=warn_pct
                                />
                                {
                                    let budget_val = budget;
                                    let warn_pct_val = warn_pct;
                                    let svc2 = svc;
                                    move || {
                                        let s = svc2.get_value().stats.get();
                                        let pec = svc2.get_value().pec_per_shot.get();
                                        let ammo_spent = s.total_shots as f64 * pec / 100.0;
                                        if let Some(budget) = budget_val {
                                            if ammo_spent >= budget * warn_pct_val / 100.0 {
                                                Some(view!{
                                                    <div class="bg-yellow-100 border border-yellow-400 text-yellow-800 rounded px-3 py-2 text-sm font-semibold ">
                                                        {format!("⚠️ Budget-Warnung: {:.0}% verbraucht ({:.2}/{:.2} PED)", ammo_spent/budget*100.0, ammo_spent, budget)}
                                                    </div>
                                                })
                                            } else { None }
                                        } else { None }
                                    }
                                }
                                <LiveStats
                                    stats=svc.get_value().stats.read_only()
                                    pec_per_shot=svc.get_value().pec_per_shot.read_only()
                                    mu_map=svc.get_value().mu_map.read_only()
                                    started_at=started_at
                                    now_sig=now_sig
                                    enhancer_cost_ped=enhancer_cost_run
                                />
                                // Feature 3: Return-Ausreißer-Warnung
                                {
                                    let creature_w = creature_for_warn.clone();
                                    let svc3 = svc;
                                    move || {
                                        let s = svc3.get_value().stats.get();
                                        let pec = svc3.get_value().pec_per_shot.get();
                                        if pec <= 0.0 || s.total_shots < 50 { return view!{<div></div>}.into_any(); }
                                        let ammo = s.total_shots as f64 * pec / 100.0;
                                        if ammo <= 0.0 { return view!{<div></div>}.into_any(); }
                                        let cur_ret = s.total_loot_value_ped / ammo * 100.0;
                                        let hist = hist_runs.get();
                                        let hist_returns: Vec<f64> = hist.iter()
                                            .filter(|r| r.config.creature == creature_w)
                                            .map(|r| r.return_pct_tt)
                                            .collect();
                                        if hist_returns.len() < 5 { return view!{<div></div>}.into_any(); }
                                        let hmean = hist_returns.iter().sum::<f64>() / hist_returns.len() as f64;
                                        let hstd = {
                                            let var = hist_returns.iter().map(|x| (x - hmean).powi(2)).sum::<f64>()
                                                / (hist_returns.len() - 1) as f64;
                                            var.sqrt()
                                        };
                                        if cur_ret < hmean - 2.0 * hstd {
                                            view!{
                                                <div class="bg-yellow-100 border border-yellow-500 text-yellow-800 rounded px-3 py-2 text-sm font-semibold ">
                                                    {format!("⚠️ Dieser Run liegt >2σ unter deinem historischen Durchschnitt ({:.1}%).", hmean)}
                                                </div>
                                            }.into_any()
                                        } else {
                                            view!{<div></div>}.into_any()
                                        }
                                    }
                                }
                                // Feature 8: Loot-Pool-Erholungs-Indikator
                                {
                                    let svc4 = svc;
                                    move || {
                                        let s = svc4.get_value().stats.get();
                                        let now = now_sig.get();
                                        let last_big = s.kill_loots.iter()
                                            .filter(|k| k.value_ped >= 5.0)
                                            .last();
                                        match last_big {
                                            Some(kl) => {
                                                let elapsed = now.saturating_sub(kl.timestamp_sec);
                                                let mins = elapsed / 60;
                                                let secs = elapsed % 60;
                                                view!{
                                                    <div class="text-xs text-gray-500 bg-gray-50 rounded px-2 py-1 ">
                                                        {format!("🎯 Letzter Big-Loot (≥5 PED): vor {}m {}s", mins, secs)}
                                                    </div>
                                                }.into_any()
                                            }
                                            None => view!{
                                                <div class="text-xs text-gray-400 bg-gray-50 rounded px-2 py-1 ">
                                                    "🎯 Noch kein Big-Loot (≥5 PED) in diesem Run"
                                                </div>
                                            }.into_any(),
                                        }
                                    }
                                }
                                <StopRecommendation stats=svc.get_value().stats.read_only() />
                                <LootItemsLive
                                    stats=svc.get_value().stats.read_only()
                                    mu_map=svc.get_value().mu_map
                                    db=db
                                    hunt_plan=hunt_plan
                                />
                                <GlobalsFeed stats=svc.get_value().stats.read_only() />
                                {move || hunt_plan.get().map(|plan| view!{
                                    <BlueprintProgress plan=plan stats=svc.get_value().stats.read_only() />
                                })}
                                <div class="space-x-2 ">
                                    <button class="btn-warning " on:click=on_pause.clone()>
                                        "⏸ Pause"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    }

                    // ── PAUSED ───────────────────────────────────────────────
                    RunState::Paused { ref config, started_at, .. } => {
                        let creature = config.creature.clone();
                        let budget = config.budget_ped;
                        let warn_pct = config.budget_warn_pct.unwrap_or(80.0);
                        let enhancer_cost_run = config.enhancer_cost_ped.unwrap_or(0.0);
                        let creature_for_warn = config.creature.clone();
                        view! {
                            <div class="space-y-3 ">
                                <div class="flex items-center gap-2 ">
                                    <span class="text-yellow-600 font-semibold ">"⏸ PAUSIERT"</span>
                                    <span class="text-sm text-gray-500 ">{creature}</span>
                                    <span class="text-xs font-mono text-gray-400 ">
                                        {fmt_elapsed(now_sig.get_untracked().saturating_sub(started_at))}
                                    </span>
                                </div>
                                <BudgetBar
                                    stats=svc.get_value().stats.read_only()
                                    pec_per_shot=svc.get_value().pec_per_shot.read_only()
                                    budget=budget warn_pct=warn_pct
                                />
                                {
                                    let budget_val = budget;
                                    let warn_pct_val = warn_pct;
                                    let svc2 = svc;
                                    move || {
                                        let s = svc2.get_value().stats.get();
                                        let pec = svc2.get_value().pec_per_shot.get();
                                        let ammo_spent = s.total_shots as f64 * pec / 100.0;
                                        if let Some(budget) = budget_val {
                                            if ammo_spent >= budget * warn_pct_val / 100.0 {
                                                Some(view!{
                                                    <div class="bg-yellow-100 border border-yellow-400 text-yellow-800 rounded px-3 py-2 text-sm font-semibold ">
                                                        {format!("⚠️ Budget-Warnung: {:.0}% verbraucht ({:.2}/{:.2} PED)", ammo_spent/budget*100.0, ammo_spent, budget)}
                                                    </div>
                                                })
                                            } else { None }
                                        } else { None }
                                    }
                                }
                                <LiveStats
                                    stats=svc.get_value().stats.read_only()
                                    pec_per_shot=svc.get_value().pec_per_shot.read_only()
                                    mu_map=svc.get_value().mu_map.read_only()
                                    started_at=started_at
                                    now_sig=now_sig
                                    enhancer_cost_ped=enhancer_cost_run
                                />
                                // Feature 3: Return-Ausreißer-Warnung
                                {
                                    let creature_w = creature_for_warn.clone();
                                    let svc3 = svc;
                                    move || {
                                        let s = svc3.get_value().stats.get();
                                        let pec = svc3.get_value().pec_per_shot.get();
                                        if pec <= 0.0 || s.total_shots < 50 { return view!{<div></div>}.into_any(); }
                                        let ammo = s.total_shots as f64 * pec / 100.0;
                                        if ammo <= 0.0 { return view!{<div></div>}.into_any(); }
                                        let cur_ret = s.total_loot_value_ped / ammo * 100.0;
                                        let hist = hist_runs.get();
                                        let hist_returns: Vec<f64> = hist.iter()
                                            .filter(|r| r.config.creature == creature_w)
                                            .map(|r| r.return_pct_tt)
                                            .collect();
                                        if hist_returns.len() < 5 { return view!{<div></div>}.into_any(); }
                                        let hmean = hist_returns.iter().sum::<f64>() / hist_returns.len() as f64;
                                        let hstd = {
                                            let var = hist_returns.iter().map(|x| (x - hmean).powi(2)).sum::<f64>()
                                                / (hist_returns.len() - 1) as f64;
                                            var.sqrt()
                                        };
                                        if cur_ret < hmean - 2.0 * hstd {
                                            view!{
                                                <div class="bg-yellow-100 border border-yellow-500 text-yellow-800 rounded px-3 py-2 text-sm font-semibold ">
                                                    {format!("⚠️ Dieser Run liegt >2σ unter deinem historischen Durchschnitt ({:.1}%).", hmean)}
                                                </div>
                                            }.into_any()
                                        } else {
                                            view!{<div></div>}.into_any()
                                        }
                                    }
                                }
                                // Feature 8: Loot-Pool-Erholungs-Indikator
                                {
                                    let svc4 = svc;
                                    move || {
                                        let s = svc4.get_value().stats.get();
                                        let now = now_sig.get();
                                        let last_big = s.kill_loots.iter()
                                            .filter(|k| k.value_ped >= 5.0)
                                            .last();
                                        match last_big {
                                            Some(kl) => {
                                                let elapsed = now.saturating_sub(kl.timestamp_sec);
                                                let mins = elapsed / 60;
                                                let secs = elapsed % 60;
                                                view!{
                                                    <div class="text-xs text-gray-500 bg-gray-50 rounded px-2 py-1 ">
                                                        {format!("🎯 Letzter Big-Loot (≥5 PED): vor {}m {}s", mins, secs)}
                                                    </div>
                                                }.into_any()
                                            }
                                            None => view!{
                                                <div class="text-xs text-gray-400 bg-gray-50 rounded px-2 py-1 ">
                                                    "🎯 Noch kein Big-Loot (≥5 PED) in diesem Run"
                                                </div>
                                            }.into_any(),
                                        }
                                    }
                                }
                                <StopRecommendation stats=svc.get_value().stats.read_only() />
                                <LootItemsLive
                                    stats=svc.get_value().stats.read_only()
                                    mu_map=svc.get_value().mu_map
                                    db=db
                                    hunt_plan=hunt_plan
                                />
                                <GlobalsFeed stats=svc.get_value().stats.read_only() />
                                {move || hunt_plan.get().map(|plan| view!{
                                    <BlueprintProgress plan=plan stats=svc.get_value().stats.read_only() />
                                })}
                                <div class="space-x-2 ">
                                    <button class="btn-primary " on:click=on_resume.clone()>
                                        "▶ Weiter"
                                    </button>
                                    <button class="btn-secondary " on:click=on_skip.clone()>
                                        "⏭ Skip to now"
                                    </button>
                                    <button class="btn-danger " on:click=on_stop.clone()>
                                        "■ Run beenden"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    }

                    // ── STOPPED ──────────────────────────────────────────────
                    RunState::Stopped { ref config, started_at, stopped_at } => {
                        let duration_secs = stopped_at.saturating_sub(started_at);
                        let creature = config.creature.clone();
                        view! {
                            <div class="space-y-3 ">
                                <div class="text-gray-700 font-semibold ">"■ RUN BEENDET"</div>
                                <div class="text-sm text-gray-500 ">
                                    {creature} " | Dauer: " {format!("{} min {} s", duration_secs / 60, duration_secs % 60)}
                                </div>
                                <RunSummary stats=svc.get_value().stats.read_only() />
                                <label class="flex flex-col gap-1 text-sm ">
                                    "Notiz (optional)"
                                    <textarea
                                        class="input "
                                        rows="2"
                                        placeholder="z.B. Loot-Buff aktiv, gutes Wetter…"
                                        on:input=move |ev| note_input.set(event_target_value(&ev))
                                    ></textarea>
                                </label>
                                <div class="space-x-2 ">
                                    <button class="btn-primary " on:click=on_save.clone()>
                                        "💾 Speichern"
                                    </button>
                                    <button class="btn-secondary " on:click=on_discard.clone()>
                                        "🗑 Verwerfen"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    }

                    // ── SAVED ────────────────────────────────────────────────
                    RunState::Saved => view! {
                        <div class="space-y-3 ">
                            <div class="text-green-600 font-semibold text-lg ">"Run gespeichert ✅"</div>
                            <button class="btn-primary " on:click=on_new_run.clone()>
                                "➕ Neuer Run"
                            </button>
                        </div>
                    }.into_any(),
                }
            }}
        </div>
    }
}

// ─── BudgetBar ───────────────────────────────────────────────────────────────

#[component]
fn BudgetBar(
    stats: ReadSignal<Stats>,
    pec_per_shot: ReadSignal<f64>,
    budget: Option<f64>,
    warn_pct: f64,
) -> impl IntoView {
    let budget = budget.unwrap_or(0.0);
    if budget <= 0.0 { return view!{<div></div>}.into_any(); }

    view! {
        <div class="text-sm ">
            {move || {
                let s = stats.get();
                let pec = pec_per_shot.get();
                // Ammo-Kosten als Basis, Fallback auf Loot-TT wenn kein Loadout
                let spent = if pec > 0.0 {
                    s.total_shots as f64 * pec / 100.0
                } else {
                    s.total_loot_value_ped
                };
                let pct = (spent / budget * 100.0).min(100.0);
                let color = if pct >= 100.0 { "bg-red-500" }
                            else if pct >= warn_pct { "bg-yellow-400" }
                            else { "bg-green-500" };
                view! {
                    <div class="flex items-center gap-2 ">
                        <span class="text-gray-500 ">"Budget:"</span>
                        <div class="flex-1 bg-gray-200 rounded-full h-3 max-w-xs ">
                            <div
                                class=format!("{color} h-3 rounded-full transition-all")
                                style=format!("width:{:.1}%", pct)
                            ></div>
                        </div>
                        <span class="text-xs font-mono ">
                            {format!("{:.0}% ({:.2}/{:.2} PED Ammo)", pct, spent, budget)}
                        </span>
                    </div>
                }
            }}
        </div>
    }.into_any()
}

// ─── LiveStats ───────────────────────────────────────────────────────────────

#[component]
fn LiveStats(
    stats: ReadSignal<Stats>,
    pec_per_shot: ReadSignal<f64>,
    mu_map: ReadSignal<std::collections::HashMap<String, f64>>,
    started_at: u64,
    now_sig: RwSignal<u64>,
    #[prop(default = 0.0)] enhancer_cost_ped: f64,
) -> impl IntoView {
    view! {
        <div class="text-sm border rounded p-3 bg-white space-y-1 ">
            <h4 class="font-semibold text-gray-700 mb-2 ">"Live-Statistiken"</h4>

            // ── Laufzeit + Tempo ─────────────────────────────────────────────
            <StatRow label="Laufzeit" value=move || fmt_elapsed(now_sig.get().saturating_sub(started_at)) />
            <StatRow label="Kills/h" value=move || {
                let elapsed = now_sig.get().saturating_sub(started_at);
                let kills = stats.get().kills;
                if elapsed > 0 && kills > 0 {
                    format!("{:.0}", kills as f64 / (elapsed as f64 / 3600.0))
                } else { "-".into() }
            } />

            // ── Kampf ────────────────────────────────────────────────────────
            <StatRow label="Kills"         value=move || stats.get().kills.to_string() />
            <StatRow label="Kosten/Kill"   value=move || {
                let s = stats.get();
                let kills = s.kills as f64;
                // Ammo-Kosten aus pec_per_shot-Signal ableiten
                let pec = pec_per_shot.get();
                if kills > 0.0 && pec > 0.0 {
                    let ammo = s.total_shots as f64 * pec / 100.0;
                    format!("{:.4} PED", ammo / kills)
                } else if kills > 0.0 {
                    "-".into()
                } else {
                    "-".into()
                }
            } />
            <StatRow label="Schüsse"       value=move || stats.get().total_shots.to_string() />
            <StatRow label="Crits"         value=move || stats.get().player_crit_hits.to_string() />
            <StatRow label="Crit-Rate"     value=move || {
                let s = stats.get();
                let shots = s.total_shots as f64;
                if shots > 0.0 { format!("{:.1}%", s.player_crit_hits as f64 / shots * 100.0) }
                else { "-".into() }
            } />
            <StatRow label="Evades"        value=move || stats.get().player_evades.to_string() />
            <StatRow label="Misses"        value=move || stats.get().player_misses.to_string() />
            <StatRow label="Gesamtschaden" value=move || format!("{:.1} pts", stats.get().total_damage) />
            <StatRow label="Ø Schaden/Schuss" value=move || {
                let s = stats.get();
                let shots = s.total_shots as f64;
                if shots > 0.0 { format!("{:.2}", s.total_damage / shots) }
                else { "-".into() }
            } />
            <StatRow label="Schaden genommen" value=move || format!("{:.1} pts", stats.get().total_damage_taken) />
            <StatRow label="Selbstheilung"    value=move || format!("{:.1} pts", stats.get().total_heal_self) />
            <StatRow label="Globals"          value=move || stats.get().globals.len().to_string() />

            // ── Kosten & Return ──────────────────────────────────────────────
            <StatRow label="Ammo-Kosten" value=move || {
                let shots = stats.get().total_shots as f64;
                let pec = pec_per_shot.get();
                if pec > 0.0 { format!("{:.4} PED", shots * pec / 100.0) }
                else { "- (kein Loadout)".into() }
            } />
            {if enhancer_cost_ped > 0.0 {
                view!{
                    <StatRow label="Enhancer-Kosten" value=move || format!("{:.4} PED", enhancer_cost_ped) />
                }.into_any()
            } else {
                view!{<span></span>}.into_any()
            }}
            <StatRow label="Loot TT"  value=move || format!("{:.4} PED", stats.get().total_loot_value_ped) />
            <StatRow label="Loot + MU" value=move || {
                let s = stats.get();
                let map = mu_map.get();
                let total: f64 = s.loot_items.iter()
                    .map(|(name, agg)| agg.total_value_ped * map.get(name).copied().unwrap_or(100.0) / 100.0)
                    .sum();
                format!("{:.4} PED", total)
            } />
            <StatRow label="Return TT" value=move || {
                let s = stats.get();
                let pec = pec_per_shot.get();
                if pec > 0.0 {
                    let ammo = s.total_shots as f64 * pec / 100.0;
                    if ammo > 0.0 { format!("{:.1}%", s.total_loot_value_ped / ammo * 100.0) }
                    else { "-".into() }
                } else { "- (kein Loadout)".into() }
            } />
            <StatRow label="Return + MU" value=move || {
                let s = stats.get();
                let pec = pec_per_shot.get();
                let map = mu_map.get();
                if pec > 0.0 {
                    let ammo = s.total_shots as f64 * pec / 100.0;
                    let loot_mu: f64 = s.loot_items.iter()
                        .map(|(name, agg)| agg.total_value_ped * map.get(name).copied().unwrap_or(100.0) / 100.0)
                        .sum();
                    if ammo > 0.0 { format!("{:.1}%", loot_mu / ammo * 100.0) }
                    else { "-".into() }
                } else { "-".into() }
            } />
            <StatRow label="Profit TT" value=move || {
                let s = stats.get();
                let pec = pec_per_shot.get();
                if pec > 0.0 {
                    let ammo = s.total_shots as f64 * pec / 100.0;
                    format!("{:+.4} PED", s.total_loot_value_ped - ammo)
                } else { "-".into() }
            } />
            <StatRow label="Profit + MU" value=move || {
                let s = stats.get();
                let pec = pec_per_shot.get();
                let map = mu_map.get();
                if pec > 0.0 {
                    let ammo = s.total_shots as f64 * pec / 100.0;
                    let loot_mu: f64 = s.loot_items.iter()
                        .map(|(name, agg)| agg.total_value_ped * map.get(name).copied().unwrap_or(100.0) / 100.0)
                        .sum();
                    format!("{:+.4} PED", loot_mu - ammo)
                } else { "-".into() }
            } />
        </div>
    }
}

fn fmt_elapsed(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 { format!("{h}h {m:02}m {s:02}s") }
    else      { format!("{m}m {s:02}s") }
}

// ─── BlueprintProgress ───────────────────────────────────────────────────────

#[component]
fn BlueprintProgress(plan: HuntPlan, stats: ReadSignal<Stats>) -> impl IntoView {
    let plan = StoredValue::new(plan);

    view! {
        <div class="border rounded bg-white ">
            <div class="px-3 py-2 border-b bg-indigo-50 flex items-center justify-between ">
                <span class="text-sm font-semibold text-indigo-800 ">
                    "📐 " {move || plan.get_value().blueprint_name.clone()}
                </span>
                <span class="text-xs text-indigo-500 ">
                    {move || format!("×{} Zyklen", plan.get_value().craft_cycles)}
                </span>
            </div>

            // Vollständigkeits-Banner
            {move || {
                let s = stats.get();
                let p = plan.get_value();
                let all_done = p.needs.iter().all(|n| {
                    let collected = s.loot_items.get(&n.item).map(|a| a.total_qty).unwrap_or(0);
                    (n.already_have + collected as f64) >= n.total_needed
                });
                if all_done && !p.needs.is_empty() {
                    view!{
                        <div class="bg-green-100 text-green-800 text-sm font-semibold px-3 py-2 ">
                            "✅ Alle Zutaten beisammen! Du kannst craften."
                        </div>
                    }.into_any()
                } else {
                    view!{<div></div>}.into_any()
                }
            }}

            // Pro Zutat: Fortschrittsbalken
            <div class="p-3 space-y-2 ">
                <For
                    each=move || plan.get_value().needs
                    key=|n| n.item.clone()
                    children=move |need| {
                        let item = need.item.clone();
                        let total_needed = need.total_needed;
                        let already_have = need.already_have;
                        view! {
                            <div>
                                <div class="flex justify-between text-xs mb-1 ">
                                    <span class="font-medium ">{item.clone()}</span>
                                    <span class="font-mono text-gray-600 ">
                                        {move || {
                                            let s = stats.get();
                                            let collected = s.loot_items.get(&item)
                                                .map(|a| a.total_qty).unwrap_or(0);
                                            let total_have = already_have + collected as f64;
                                            format!("{:.0} / {:.0} (diesen Run: +{})",
                                                total_have, total_needed, collected)
                                        }}
                                    </span>
                                </div>
                                <div class="bg-gray-200 rounded-full h-3 ">
                                    {move || {
                                        let item2 = need.item.clone();
                                        let s = stats.get();
                                        let collected = s.loot_items.get(&item2)
                                            .map(|a| a.total_qty).unwrap_or(0);
                                        let total_have = already_have + collected as f64;
                                        let pct = if total_needed > 0.0 {
                                            (total_have / total_needed * 100.0).min(100.0)
                                        } else { 100.0 };
                                        let color = if pct >= 100.0 { "bg-green-500" }
                                                    else if pct >= 60.0 { "bg-blue-400" }
                                                    else { "bg-indigo-400" };
                                        view! {
                                            <div
                                                class=format!("{color} h-3 rounded-full transition-all")
                                                style=format!("width:{:.1}%", pct)
                                            ></div>
                                        }
                                    }}
                                </div>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}

// ─── LootItemsLive ───────────────────────────────────────────────────────────
// Zeigt Loot-Items mit inline-MU%-Bearbeitung während des Runs.
// Verwendet <For> damit Signal pro Item stabil bleibt (kein Dispose bei Stats-Update).

#[component]
fn LootItemsLive(
    stats: ReadSignal<Stats>,
    mu_map: RwSignal<std::collections::HashMap<String, f64>>,
    db: ReadSignal<Option<Db>, leptos::prelude::LocalStorage>,
    hunt_plan: ReadSignal<Option<HuntPlan>, leptos::prelude::LocalStorage>,
) -> impl IntoView {
    view! {
        <div class="mt-2 border rounded bg-white ">
            <div class="flex items-center justify-between px-3 py-2 border-b bg-gray-50 ">
                <span class="text-sm font-semibold text-gray-700 ">"Loot-Items (MU justierbar)"</span>
                <span class="text-xs text-gray-400 ">"💾 = sofort speichern & neu berechnen"</span>
            </div>
            {move || {
                let s = stats.get();
                if s.loot_items.is_empty() {
                    return view!{<div class="p-3 text-gray-400 text-xs italic ">"Noch kein Loot..."</div>}.into_any();
                }
                // Sortierte Item-Liste für <For>
                let mut items: Vec<(String, LootItemAgg)> = s.loot_items.into_iter().collect();
                items.sort_by(|a, b| b.1.total_value_ped.partial_cmp(&a.1.total_value_ped)
                    .unwrap_or(std::cmp::Ordering::Equal));
                view! {
                    <table class="w-full text-xs border-collapse ">
                        <thead class="border-b ">
                            <tr class="text-gray-500 ">
                                <th class="text-left px-3 py-1 ">"Item"</th>
                                <th class="text-right px-3 py-1 ">"TT"</th>
                                <th class="text-right px-3 py-1 ">"Anz."</th>
                                <th class="text-right px-2 py-1 ">"MU%"</th>
                                <th class="text-right px-3 py-1 ">"Wert+MU"</th>
                                <th class="px-1 py-1 "></th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || {
                                    let s = stats.get();
                                    let mut v: Vec<(String, LootItemAgg)> = s.loot_items.into_iter().collect();
                                    v.sort_by(|a, b| b.1.total_value_ped.partial_cmp(&a.1.total_value_ped)
                                        .unwrap_or(std::cmp::Ordering::Equal));
                                    v
                                }
                                key=|(name, _)| name.clone()
                                children=move |(name, _init_agg)| {
                                    // Signal für MU%-Input – stabil weil <For> das Child nur 1x erstellt
                                    let mu_init = mu_map.get_untracked().get(&name).copied().unwrap_or(100.0);
                                    let mu_sig = RwSignal::new(mu_init.to_string());
                                    // Live-Werte aus stats
                                    let name_for_agg = name.clone();
                                    let live_agg = Signal::derive(move || {
                                        stats.get().loot_items.get(&name_for_agg).copied()
                                            .unwrap_or_default()
                                    });
                                    let name_save = name.clone();
                                    let mu_for_badge = mu_map.get_untracked().get(&name).copied().unwrap_or(100.0);
                                    let hp = hunt_plan.get_untracked();
                                    let is_needed = hp.as_ref().map(|p| p.needs.iter().any(|n| n.item == name)).unwrap_or(false);
                                    view! {
                                        <tr class="border-b last:border-0 hover:bg-gray-50 ">
                                            <td class="px-3 py-1 font-medium ">
                                                {name.clone()}
                                                {if is_needed {
                                                    view!{<span class="ml-1 text-green-600 font-bold text-xs ">"🔒 Halten"</span>}.into_any()
                                                } else if mu_for_badge >= 200.0 {
                                                    view!{<span class="ml-1 text-green-600 font-bold ">"🔥"</span>}.into_any()
                                                } else if mu_for_badge >= 150.0 {
                                                    view!{<span class="ml-1 ">"💰 🔔"</span>}.into_any()
                                                } else {
                                                    view!{<span></span>}.into_any()
                                                }}
                                            </td>
                                            <td class="text-right px-3 py-1 font-mono ">
                                                {move || format!("{:.4}", live_agg.get().total_value_ped)}
                                            </td>
                                            <td class="text-right px-3 py-1 ">
                                                {move || live_agg.get().event_count.to_string()}
                                            </td>
                                            <td class="px-2 py-1 ">
                                                <input
                                                    class="input w-20 text-right py-0 text-xs "
                                                    type="number" step="0.1" min="100"
                                                    value=move || mu_sig.get()
                                                    on:input=move |ev| mu_sig.set(event_target_value(&ev))
                                                />
                                            </td>
                                            <td class="text-right px-3 py-1 font-mono ">
                                                {move || {
                                                    let tt = live_agg.get().total_value_ped;
                                                    let mu: f64 = mu_sig.get().parse().unwrap_or(100.0);
                                                    format!("{:.4}", tt * mu / 100.0)
                                                }}
                                            </td>
                                            <td class="px-1 py-1 ">
                                                <button class="btn-xs " on:click={
                                                    let name = name_save.clone();
                                                    move |_| {
                                                        let mu: f64 = mu_sig.get_untracked().parse().unwrap_or(100.0);
                                                        // mu_map sofort aktualisieren (tailer liest nächste Sekunde aus DB)
                                                        mu_map.update(|m| { m.insert(name.clone(), mu); });
                                                        if let Some(d) = db.get_untracked() {
                                                            let name2 = name.clone();
                                                            spawn_local(async move {
                                                                // Droppers nicht verlieren
                                                                let droppers = if let Ok(all) = d.get_all_loot_item_configs().await {
                                                                    all.into_iter().find(|c| c.name == name2)
                                                                        .map(|c| c.droppers).unwrap_or_default()
                                                                } else { vec![] };
                                                                let _ = d.save_loot_item_config(&LootItemConfig {
                                                                    name: name2, mu_pct: mu, droppers,
                                                                    last_updated_sec: Some(crate::services::run_service::now_sec()),
                                                                }).await;
                                                            });
                                                        }
                                                    }
                                                }>"💾"</button>
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}

// ─── GlobalsFeed ─────────────────────────────────────────────────────────────

#[component]
fn GlobalsFeed(stats: ReadSignal<Stats>) -> impl IntoView {
    view! {
        <div>
            {move || {
                let globals = stats.get().globals;
                if globals.is_empty() {
                    return view!{<div></div>}.into_any();
                }
                let last5: Vec<_> = globals.iter().rev().take(5).cloned().collect();
                view! {
                    <div class="border-2 border-yellow-400 rounded p-2 ">
                        <div class="font-bold mb-1 ">"🌟 Globals"</div>
                        <div class="space-y-1 text-sm ">
                            {last5.into_iter().enumerate().map(|(i, g)| {
                                let text = format!("🌟 {} | {:.2} PED | {}", g.creature, g.value_ped, g.player);
                                if i == 0 {
                                    view!{<div class="bg-yellow-50 rounded px-1 ">{text}</div>}.into_any()
                                } else {
                                    view!{<div class="text-gray-500 px-1 ">{text}</div>}.into_any()
                                }
                            }).collect_view()}
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ─── RunSummary ───────────────────────────────────────────────────────────────

#[component]
fn RunSummary(stats: ReadSignal<Stats>) -> impl IntoView {
    view! {
        <div class="text-sm border rounded p-3 bg-gray-50 space-y-1 ">
            <h4 class="font-semibold mb-2 ">"Abschluss-Zusammenfassung"</h4>
            <StatRow label="Kills"      value=move || stats.get().kills.to_string() />
            <StatRow label="Schüsse"    value=move || stats.get().total_shots.to_string() />
            <StatRow label="Loot TT"    value=move || format!("{:.4} PED", stats.get().total_loot_value_ped) />
            <StatRow label="Gesamtschaden" value=move || format!("{:.1} pts", stats.get().total_damage) />
            <StatRow label="Schaden genommen" value=move || format!("{:.1} pts", stats.get().total_damage_taken) />
            <StatRow label="Heilung"    value=move || format!("{:.1} pts", stats.get().total_heal_self) />

            // Loot-Tabelle
            {move || {
                let s = stats.get();
                if s.loot_items.is_empty() {
                    view!{<div class="text-gray-400 italic ">"Kein Loot"</div>}.into_any()
                } else {
                    let mut items: Vec<_> = s.loot_items.iter().collect();
                    items.sort_by(|a, b| b.1.total_value_ped.partial_cmp(&a.1.total_value_ped).unwrap_or(std::cmp::Ordering::Equal));
                    view! {
                        <table class="mt-2 text-xs w-full ">
                            <thead class="border-b ">
                                <tr>
                                    <th class="text-left pr-3 ">"Item"</th>
                                    <th class="text-right pr-3 ">"TT-Wert"</th>
                                    <th class="text-right ">"Anzahl"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {items.into_iter().map(|(name, agg)| {
                                    let name = name.clone();
                                    let tt = format!("{:.4}", agg.total_value_ped);
                                    let cnt = agg.event_count.to_string();
                                    view! {
                                        <tr class="border-b last:border-0 ">
                                            <td class="pr-3 ">{name}</td>
                                            <td class="text-right pr-3 ">{tt}</td>
                                            <td class="text-right ">{cnt}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
        </div>
    }
}

// ─── StatRow ─────────────────────────────────────────────────────────────────

#[component]
fn StatRow<F>(label: &'static str, value: F) -> impl IntoView
where
    F: Fn() -> String + 'static + Send,
{
    view! {
        <div class="flex justify-between ">
            <span class="text-gray-600 ">{label}</span>
            <span class="font-mono tabular-nums ">{value}</span>
        </div>
    }
}

// ─── StopRecommendation ───────────────────────────────────────────────────────

#[component]
fn StopRecommendation(stats: ReadSignal<Stats>) -> impl IntoView {
    view! {
        <div>
            {move || {
                let s = stats.get();
                let all_loots: Vec<f64> = s.kill_loots.iter().map(|k| k.value_ped).collect();
                let total_kills = all_loots.len();
                if total_kills < 10 {
                    return view!{<div></div>}.into_any();
                }
                let all_mean = all_loots.iter().sum::<f64>() / all_loots.len() as f64;
                let recent_count = total_kills.min(20);
                let recent_loots: Vec<f64> = all_loots.iter().rev().take(recent_count).cloned().collect();
                let recent_mean = recent_loots.iter().sum::<f64>() / recent_loots.len().max(1) as f64;

                let (icon, text, bg_cls) = if recent_mean >= all_mean * 1.1 {
                    ("🟢", "Lauf läuft gut — weitermachen!", "bg-green-50 border-green-200 text-green-800")
                } else if recent_mean < all_mean * 0.8 {
                    ("🔴", "Schlechte Phase — Pause erwägen?", "bg-red-50 border-red-200 text-red-800")
                } else {
                    ("🟡", "Normal", "bg-yellow-50 border-yellow-200 text-yellow-800")
                };

                view! {
                    <div class=format!("border rounded px-3 py-2 text-sm {}", bg_cls)>
                        <span class="font-semibold ">"📍 Stop-Empfehlung: "</span>
                        {icon} " " {text}
                        <span class="text-xs ml-2 opacity-70 ">
                            {format!("(Letzte {} Kills: Ø {:.4} PED, Gesamt-Ø: {:.4} PED)", recent_count, recent_mean, all_mean)}
                        </span>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ─── Hilfsfunktionen ─────────────────────────────────────────────────────────

async fn get_file_size(handle: &JsValue) -> u64 {
    let file_js = if Reflect::has(handle, &JsValue::from_str("getFile")).unwrap_or(false) {
        match getFileFromHandle(handle).await {
            Ok(f) => f,
            Err(_) => return 0,
        }
    } else {
        handle.clone()
    };
    Reflect::get(&file_js, &JsValue::from_str("size"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64
}
