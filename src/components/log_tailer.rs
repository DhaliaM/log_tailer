use crate::analytics::stats::Stats;
use crate::persistence::idb::Db;
use crate::services::{fs_access, tailer};
use js_sys::{Boolean, Reflect};
use leptos::prelude::{
    event_target_value, on_cleanup, signal_local, ClassAttribute, Effect, ElementChild, For, Get,
    OnAttribute, ReadSignal, RwSignal, Set, Show, Update, GetUntracked,
};
use leptos::task::spawn_local;
use leptos::*;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wasm_bindgen::JsValue;

#[component]
pub fn LogTailer() -> impl IntoView {
    // Nicht-Send-Werte => signal_local!
    let (handle, set_handle) = signal_local::<Option<JsValue>>(None);
    let (db, set_db) = signal_local::<Option<Db>>(None);

    // Send/Sync-Werte können in normalen RwSignals liegen
    let offset = RwSignal::new(0.0_f64);
    let stats = RwSignal::new(Stats::default());
    let started = RwSignal::new(false);
    let pec_per_shot = RwSignal::new(19.6548_f64);
    let mu_map = RwSignal::new(HashMap::<String, f64>::new());

    let running = RwSignal::new(false);

    let dbg_lines = RwSignal::new(0_u64);
    let dbg_events = RwSignal::new(0_u64);

    let ui_msg = RwSignal::new(String::new());
    let start_clicks = RwSignal::new(0_u64);

    // Nicht-reaktive Flags
    let running_flag = Arc::new(AtomicBool::new(false));
    let cancel_token = Arc::new(AtomicBool::new(false));

    // Laufendes Signal -> Flag spiegeln
    {
        let running = running.clone();
        let running_flag = running_flag.clone();
        Effect::new(move |_| {
            running_flag.store(running.get(), Ordering::Relaxed);
        });
    }

    // Beim Unmount Task beenden
    on_cleanup({
        let cancel_token = cancel_token.clone();
        move || {
            cancel_token.store(true, Ordering::Relaxed);
        }
    });

    let on_start = {
        let running = running.clone();
        let handle = handle.clone();
        let offset = offset.clone();
        let stats = stats.clone();
        // optional: kleine Statusmeldung, falls du ein ui_msg Signal hast
        // let ui_msg = ui_msg.clone();

        move |_| {
            spawn_local(async move {
                if let Some(h) = handle.get_untracked() {
                    // Handle kann FileSystemFileHandle ODER direkt File/Blob sein
                    let file_js =
                        if Reflect::has(&h, &JsValue::from_str("getFile")).unwrap_or(false) {
                            match crate::services::fs_access::getFileFromHandle(&h).await {
                                Ok(f) => f,
                                Err(e) => {
                                    log::warn!("getFileFromHandle: {:?}", e);
                                    return;
                                }
                            }
                        } else {
                            h.clone()
                        };

                    let size = Reflect::get(&file_js, &JsValue::from_str("size"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    // 1) Run am Dateiende verankern
                    offset.set(size);
                    // 2) Run-Stats zurücksetzen (global lässt du ggf. separat weiterzählen)
                    stats.set(Stats::default());
                    // 3) Los geht’s
                    running.set(true);
                    // ui_msg.set(format!("Run start @ {} bytes", size));
                } else {
                    log::warn!("Start geklickt, aber kein Handle gewählt");
                    // ui_msg.set("Kein Handle – zuerst Datei wählen".into());
                }
            });
        }
    };

    let on_stop = {
        let running = running.clone();
        let ui_msg = ui_msg.clone();
        move |_| {
            running.set(false);
            ui_msg.set("Stop clicked".into());
        }
    };

    // DB einmalig öffnen (clientseitig)
    Effect::new({
        let set_db = set_db.clone();
        move |_| {
            spawn_local(async move {
                match Db::open("logtailer_db", 1).await {
                    Ok(d) => set_db.set(Some(d)),
                    Err(e) => log::warn!("DB open: {:?}", e),
                }
            });
        }
    });

    // Datei auswählen
    let on_pick = {
        let set_handle = set_handle.clone();
        let offset = offset.clone();
        let ui_msg = ui_msg.clone();
        move |_| {
            // WICHTIG: leptos::task::spawn_local verwenden (nicht wasm_bindgen_futures)
            spawn_local(async move {
                // 1) Prüfen, ob moderner Picker da ist
                let supports_picker = fs_access::hasOpenFilePicker().unwrap_or(false);

                // 2) Versuche modernen Picker (Handle) ODER Fallback (Blob/File)
                let picked = if supports_picker {
                    match fs_access::pickFile().await {
                        Ok(h) => {
                            ui_msg.set("Picker: got handle".into());
                            Some(h)
                        }
                        Err(e) => {
                            log::warn!("pickFile error: {:?}", e);
                            ui_msg.set("Picker: error (see console)".into());
                            None
                        }
                    }
                } else {
                    // Fallback: <input type=file>
                    match fs_access::simpleInputPick().await {
                        Ok(file_blob) => {
                            ui_msg.set("Fallback input: got File/Blob".into());
                            Some(file_blob)
                        }
                        Err(e) => {
                            log::warn!("simpleInputPick error: {:?}", e);
                            ui_msg.set("Fallback input: error (see console)".into());
                            None
                        }
                    }
                };

                // 3) Wenn irgendwas gewählt wurde: Handle/Blob setzen und Offset auf 0
                if let Some(h) = picked {
                    if supports_picker {
                        match fs_access::ensureReadPermission(&h).await {
                            Ok(v) => {
                                let granted = Boolean::from(v).value_of(); // JsValue -> bool
                                if granted {
                                    ui_msg.set("Permission: granted".into());
                                } else {
                                    ui_msg.set("Permission: denied (will still try)".into());
                                }
                            }
                            Err(e) => {
                                log::warn!("ensureReadPermission error: {:?}", e);
                                ui_msg.set("Permission: error (see console)".into());
                            }
                        }
                    }
                    set_handle.set(Some(h));
                    offset.set(0.0);
                }
            });
        }
    };

    // Polling starten, sobald Handle + DB da sind (und noch nicht gestartet)
        // Polling starten, sobald Handle + DB da sind (und noch nicht gestartet)
            Effect::new({
                let handle = handle.clone();
                let db = db.clone();
                let offset = offset.clone();
                let stats = stats.clone();
                let started = started.clone();
                let running_flag = running_flag.clone();
                let cancel_token = cancel_token.clone();
                move |_| {
                    // started ist selbst ein Signal → das ist okay
                    if started.get() {
                        return;
                    }

                    // ⬇️ hier reaktiv im Effekt lesen, nicht außerhalb
                    let h_opt = handle.get();
                    let d_opt = db.get();

                    if let (Some(h), Some(d)) = (h_opt, d_opt) {
                        started.set(true);
                        tailer::start_polling(
                            h.clone(),
                            offset,
                            stats,
                            d.clone(),
                            running_flag.clone(),
                            cancel_token.clone(),
                            dbg_lines,
                            dbg_events,
                        );
                    }
                }
            });




    view! {
      <div class="mt-4 space-x-2">
        <button on:click=on_pick>"Log-Datei auswählen"</button>
      </div>

      <div class="mt-3 space-x-2">
          <button on:click=on_start disabled=move || running.get()>"Start"</button>
          <button on:click=on_stop  disabled=move || !running.get()>"Stop"</button>
          <span class="ml-2 text-xs opacity-70">{move || if running.get() {"(läuft)"} else {"(pausiert)"}}</span>
      </div>
      <div class="mt-2 text-xs opacity-70">
          {"Handle: "}{move || if handle.get().is_some() { "ok" } else { "—" }}
          {"  |  DB: "}{move || if db.get().is_some() { "ok" } else { "—" }}
          {"  |  Running: "}{move || if running.get() { "true" } else { "false" }}
          {"  |  DBG lines: "}{move || dbg_lines.get()}
          {"  |  DBG events: "}{move || dbg_events.get()}
      </div>

      <div class="mt-4 grid gap-2 sm:grid-cols-2 max-w-sm">
        <label>
            "PEC per Shot:"
            <input
                type="number"
                step="0.0001"
                value=move || format!("{:.4}", pec_per_shot.get())
                on:input=move |ev| {
                    let v = event_target_value(&ev).parse::<f64>().unwrap_or(0.0);
                    pec_per_shot.set(v);
                }
            />
        </label>
      </div>


      <div class="mt-4">
        // db (local signal) vorhanden?
        <Show when=move || db.get().is_some()>
          <StatsView
              stats=stats.read_only()
              pec_per_shot=pec_per_shot.read_only()
              mu_map=mu_map
          />
        </Show>
      </div>
    }
}

#[component]
pub fn StatsView(
    stats: ReadSignal<Stats>,
    pec_per_shot: ReadSignal<f64>,
    mu_map: RwSignal<HashMap<String, f64>>,
) -> impl IntoView {
    let s = move || stats.get();

    let shots = move || s().player_attacks() as f64;
    let misses = move || s().player_misses;
    let costs_ped = move || (shots() * pec_per_shot.get()) / 100.0;
    let loot_sum = move || s().total_loot_value_ped;
    let total_loot_events = move || {
        s().loot_items
            .values()
            .map(|li| li.event_count)
            .sum::<u64>()
    };

    let item_names_sorted = move || {
        let s = stats.get();
        let mut v: Vec<(String, f64)> = s
            .loot_items
            .iter()
            .map(|(name, li)| (name.clone(), li.total_value_ped))
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(name, _)| name).collect::<Vec<_>>()
    };

    let rows = move || {
        let costs = costs_ped();

        // ⬇️ diese Gets machen rows() reaktiv auf beide Quellen
        let mu_snapshot = mu_map.get(); // HashMap<String,f64>

        // HashMap -> Vec (für Anzeige/Sortierung)
        let mut v: Vec<(String, f64, u64)> = s()
            .loot_items
            .iter()
            .map(|(name, li)| (name.clone(), li.total_value_ped, li.event_count))
            .collect();

        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let denom = s().kills as f64;

        v.into_iter()
            .map(move |(name, total_value, ev_count)| {
                let pct = if denom > 0.0 {
                    (ev_count as f64 / denom) * 100.0
                } else {
                    0.0
                };

                // ⬇️ Item-spezifischer MU-Wert (Fallback: 100.0)
                let item_mu = mu_snapshot.get(&name).copied().unwrap_or(100.0); // % (z.B. 110)
                let factor = item_mu / 100.0;

                let sale_value = total_value * factor;
                let ratio = if costs > 0.0 {
                    total_value / costs
                } else {
                    0.0
                };

                // Wir geben den item_mu mit zurück, damit der Row-Input ihn anzeigen kann
                (name, total_value, ev_count, pct, ratio, sale_value, item_mu)
            })
            .collect::<Vec<_>>()
    };

    // Gesamt-TT mit MU (Fallback 100%)
    let total_loot_mu = move || {
        let mu = mu_map.get(); // HashMap<String, f64>
        let s = stats.get();
        s.loot_items
            .iter()
            .map(|(name, li)| {
                let item_mu = mu.get(name).copied().unwrap_or(100.0); // % → 100 = TT
                li.total_value_ped * (item_mu / 100.0)
            })
            .sum::<f64>()
    };

    // Optional: ROI auf MU-Basis
    let roi_mu = move || {
        let costs = (stats.get().player_attacks() as f64 * pec_per_shot.get()) / 100.0;
        let loot = total_loot_mu();
        if costs > 0.0 {
            (loot - costs) / costs
        } else {
            0.0
        }
    };

    let return_in_percent = move || {
        let costs = (stats.get().player_attacks() as f64 * pec_per_shot.get()) / 100.0;
        let loot = loot_sum();
        if costs > 0.0 {
            (loot / costs) * 100.0
        } else {
            0.0
        }
    };

    let return_in_percent_mu = move || {
        let costs = (stats.get().player_attacks() as f64 * pec_per_shot.get()) / 100.0;
        let loot = total_loot_mu();
        if costs > 0.0 {
            (loot / costs) * 100.0
        } else {
            0.0
        }
    };

    let dpp = move || {
        let s = stats.get();
        let shots = s.player_attacks() as f64;
        let pec   = pec_per_shot.get();
        if shots > 0.0 && pec > 0.0 {
            s.total_damage / (shots * pec)
        } else { 0.0 }
    };

    view! {
      <div class="text-sm space-y-2">
        <div>"Kills: " {move || s().kills}</div>
        <div>"Schüsse (≈): " {move || shots() as u64}</div>
        <div>"Crits: " {move || s().player_crit_hits}</div>
        <div>"Evades: " {move || s().player_evades}</div>
        <div>"Misses: " {move || misses()}</div>
        <div>"Gesamtkosten: " {move || format!("{:.4} PED", costs_ped())}</div>
        <div>"Gesamt-Loot (TT): " {move || format!("{:.4} PED", loot_sum())}</div>
        <div>"Gesamt-Loot mit MU" {move ||format!("{:.4}", total_loot_mu())}</div>
        <div>"Return (%): " {move || format!("{:.1}%", return_in_percent())}</div>
        <div>"Return (% MU): " {move || format!("{:.1}%", return_in_percent_mu())}</div>
        <div>"ROI (MU): " {move || format!("{:.1}%", roi_mu() * 100.0)}</div>
        <div>"Average cost per kill: " {move || format!("{:.4} PED", costs_ped() / total_loot_events() as f64)}</div>
        <div>"Damage / PEC: " {move || format!("{:.3}", dpp())}</div>
      </div>

      <div class="mt-3 overflow-x-auto">
        <table class="min-w-[32rem] text-sm">
          <thead class="text-left border-b">
              <tr>
                <th class="pr-3 py-1">Item</th>
                <th class="pr-3 py-1">TT-Wert (PED)</th>
                <th class="pr-3 py-1">% d. Kills</th>
                <th class="pr-3 py-1">TT / Kosten</th>
                <th class="pr-3 py-1">Wert @ Faktor</th>
                <th class="pr-3 py-1">MU %</th>
              </tr>
          </thead>
         <tbody>
              <For
                each=move || item_names_sorted()
                key=|name| name.clone()
                children=move |name: String| {
                  // pro Zelle eigene Name-Klone (verhindert moved-value-Fehler)
                  let name_tt     = name.clone();
                  let name_pct    = name.clone();
                  let name_ratio  = name.clone();
                  let name_sale   = name.clone();
                  let name_mu_val = name.clone();
                  let name_mu_key = name.clone();
                  let display_name = name; // Original nur für die Anzeige

                  // TT (reaktiv)
                  let tt_cell = move || {
                    stats.get()
                      .loot_items
                      .get(&name_tt)
                      .map(|li| li.total_value_ped)
                      .unwrap_or(0.0)
                  };

                  // % d. Kills (reaktiv)
                  let pct_cell = move || {
                    let kills = stats.get().kills as f64;
                    let evs = stats.get()
                      .loot_items
                      .get(&name_pct)
                      .map(|li| li.event_count as f64)
                      .unwrap_or(0.0);
                    if kills > 0.0 { (evs / kills) * 100.0 } else { 0.0 }
                  };

                  // TT/Kosten (reaktiv)
                  let ratio_cell = move || {
                    let shots = stats.get().player_attacks() as f64;
                    let costs = (shots * pec_per_shot.get()) / 100.0; // PEC → PED
                    let tt = stats.get()
                      .loot_items
                      .get(&name_ratio)
                      .map(|li| li.total_value_ped)
                      .unwrap_or(0.0);
                    if costs > 0.0 { tt / costs } else { 0.0 }
                  };

                  // Verkauf @ MU (reaktiv, Fallback 100%)
                  let sale_cell = move || {
                    let tt = stats.get()
                      .loot_items
                      .get(&name_sale)
                      .map(|li| li.total_value_ped)
                      .unwrap_or(0.0);
                    let mu = mu_map
                      .get()
                      .get(&name_sale)
                      .copied()
                      .unwrap_or(100.0); // kein globaler MU → 100%
                    tt * (mu / 100.0)
                  };

                  // MU%-Input für dieses Item
                  let on_mu_input = {
                    let mu_map = mu_map.clone();
                    move |ev: leptos::ev::Event| {
                      if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                        mu_map.update(|m| { m.insert(name_mu_key.clone(), v); });
                      }
                    }
                  };

                  view! {
                    <tr class="border-b last:border-b-0">
                      <td class="pr-3 py-1">{display_name.clone()}</td>
                      <td class="pr-3 py-1">{move || format!("{:.4}", tt_cell())}</td>
                      <td class="pr-3 py-1">{move || format!("{:.1}%", pct_cell())}</td>
                      <td class="pr-3 py-1">
                        {move || {
                          let r = ratio_cell();
                          if r.is_finite() { format!("{:.3}", r) } else { "-".into() }
                        }}
                      </td>
                      <td class="pr-3 py-1">{move || format!("{:.4}", sale_cell())}</td>
                      <td class="pr-3 py-1">
                        <input
                          class="w-16"
                          type="number"
                          step="1"
                          // aktueller MU für dieses Item (Fallback 100)
                          value=move || {
                            let mu = mu_map.get().get(&name_mu_val).copied().unwrap_or(100.0);
                            format!("{:.0}", mu)
                          }
                          on:input=on_mu_input
                        />
                      </td>
                    </tr>
                  }
                }
              />
            </tbody>
        </table>
      </div>
    }
}
