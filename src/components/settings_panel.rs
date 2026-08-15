use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::*;
use wasm_bindgen::JsValue;

use crate::domain::types::SavedRun;
use crate::persistence::idb::Db;
use crate::persistence::export::{export_all, export_runs_csv, import_all, trigger_download, trigger_download_text, ConflictStrategy};

#[component]
pub fn SettingsPanel() -> impl IntoView {
    let (db, set_db) = signal_local::<Option<Db>>(None);
    let player_name = RwSignal::new(String::new());
    let saved_msg   = RwSignal::new(String::new());
    let import_preview = RwSignal::new(String::new());
    let import_json = RwSignal::new(String::new());
    let ped_per_skillpoint = RwSignal::new(String::new());
    // Format: "Evader=450,Longblades=320" — kommasepariert, Skill=Punkte
    let skill_levels_str   = RwSignal::new(String::new());

    // PED-Card
    let (runs_for_ped, set_runs_for_ped) = signal_local::<Vec<SavedRun>>(vec![]);
    let ped_starting = RwSignal::new(String::new()); // manuelle Startbilanz

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await {
                if let Ok(Some(name)) = d.get_setting("player_name").await {
                    player_name.set(name);
                }
                if let Ok(Some(start)) = d.get_setting("ped_starting").await {
                    ped_starting.set(start);
                }
                if let Ok(Some(v)) = d.get_setting("ped_per_skillpoint").await {
                    ped_per_skillpoint.set(v);
                }
                if let Ok(Some(v)) = d.get_setting("skill_levels").await {
                    skill_levels_str.set(v);
                }
                if let Ok(rs) = d.get_all_runs().await {
                    set_runs_for_ped.set(rs);
                }
                set_db.set(Some(d));
            }
        });
    });

    // Speichern
    let on_save_name = {
        let db = db.clone();
        move |_| {
            if let Some(d) = db.get_untracked() {
                let name = player_name.get_untracked();
                spawn_local(async move {
                    if let Err(e) = d.set_setting("player_name", &name).await {
                        log::warn!("set_setting: {:?}", e);
                    }
                    saved_msg.set("✅ Gespeichert".into());
                });
            }
        }
    };

    // Export
    let on_export = {
        let db = db.clone();
        move |_| {
            if let Some(d) = db.get_untracked() {
                spawn_local(async move {
                    match export_all(&d).await {
                        Ok(json) => {
                            let now = js_sys::Date::now() as u64;
                            let filename = format!("logtailer_backup_{}.json", now);
                            trigger_download(&filename, &json);
                        }
                        Err(e) => log::warn!("export_all: {:?}", e),
                    }
                });
            }
        }
    };

    // CSV Export
    let on_export_csv = {
        let db = db.clone();
        move |_| {
            if let Some(d) = db.get_untracked() {
                spawn_local(async move {
                    match export_runs_csv(&d).await {
                        Ok(csv) => {
                            let now = js_sys::Date::now() as u64;
                            let filename = format!("logtailer_runs_{}.csv", now);
                            trigger_download_text(&filename, &csv, "text/csv");
                        }
                        Err(e) => log::warn!("export_runs_csv: {:?}", e),
                    }
                });
            }
        }
    };

    // Import: Datei einlesen
    let on_import_pick = move |ev: leptos::ev::Event| {
        use wasm_bindgen::JsCast;
        let input = ev.target().unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
        if let Some(files) = input.files() {
            if let Some(file) = files.get(0) {
                use wasm_bindgen_futures::JsFuture;
                let file_js: JsValue = file.into();
                spawn_local(async move {
                    let blob: web_sys::Blob = file_js.dyn_into().unwrap();
                    let text = JsFuture::from(blob.text()).await.unwrap();
                    let json = text.as_string().unwrap_or_default();
                    // Vorschau
                    if let Ok(backup) = serde_json::from_str::<serde_json::Value>(&json) {
                        let runs  = backup["runs"].as_array().map(|a| a.len()).unwrap_or(0);
                        let creat = backup["creatures"].as_array().map(|a| a.len()).unwrap_or(0);
                        let bps   = backup["blueprints"].as_array().map(|a| a.len()).unwrap_or(0);
                        import_preview.set(format!("{} Runs, {} Kreaturen, {} Blueprints", runs, creat, bps));
                    }
                    import_json.set(json);
                });
            }
        }
    };

    let on_import_confirm = {
        let db = db.clone();
        move |_| {
            let json = import_json.get_untracked();
            if json.is_empty() { return; }
            if let Some(d) = db.get_untracked() {
                spawn_local(async move {
                    match import_all(&d, &json, ConflictStrategy::Skip).await {
                        Ok(summary) => {
                            saved_msg.set(format!(
                                "Import abgeschlossen: {} Runs, {} Kreaturen, {} Loadouts importiert.",
                                summary.runs_imported, summary.creatures_imported, summary.loadouts_imported
                            ));
                            import_preview.set(String::new());
                            import_json.set(String::new());
                        }
                        Err(e) => saved_msg.set(format!("Import-Fehler: {:?}", e)),
                    }
                });
            }
        }
    };

    // Daten löschen
    let on_reset = move |_| {
        let ok = web_sys::window()
            .and_then(|w| w.confirm_with_message("ALLE Daten unwiderruflich löschen?").ok())
            .unwrap_or(false);
        if ok {
            let window = web_sys::window().unwrap();
            let idb_factory = window.indexed_db().unwrap().unwrap();
            let _ = idb_factory.delete_database("log_tailer");
            window.location().reload().unwrap_or_default();
        }
    };

    view! {
        <div class="space-y-6 max-w-lg ">
            <h2 class="text-lg font-semibold ">"⚙️ Einstellungen"</h2>

            // Statusmeldung
            {move || (!saved_msg.get().is_empty()).then(|| view!{
                <div class="p-2 bg-green-50 border border-green-300 text-sm rounded text-green-800 ">
                    {move || saved_msg.get()}
                </div>
            })}

            // Charakter-Name
            <section class="space-y-2 ">
                <h3 class="font-medium ">"Charakter-Name"</h3>
                <div class="flex gap-2 ">
                    <input class="input flex-1 "
                        placeholder="z.B. Mordekai Azrael"
                        value=move || player_name.get()
                        on:input=move |ev| player_name.set(event_target_value(&ev))
                    />
                    <button class="btn-primary " on:click=on_save_name>"💾 Speichern"</button>
                </div>
                <p class="text-xs text-gray-500 ">"Wird für Globals-Filterung verwendet (nur eigene Globals zählen)."</p>
            </section>

            // Export
            <section class="space-y-2 ">
                <h3 class="font-medium ">"Backup exportieren"</h3>
                <div class="flex gap-2 flex-wrap ">
                    <button class="btn-primary " on:click=on_export>
                        "⬇ Backup herunterladen (JSON)"
                    </button>
                    <button class="btn-secondary " on:click=on_export_csv>
                        "⬇ Runs als CSV"
                    </button>
                </div>
            </section>

            // Import
            <section class="space-y-2 ">
                <h3 class="font-medium ">"Backup importieren"</h3>
                <input type="file" accept=".json" on:change=on_import_pick />
                {move || (!import_preview.get().is_empty()).then(|| view!{
                    <div class="p-2 bg-blue-50 border border-blue-200 text-sm rounded ">
                        "Vorschau: " {move || import_preview.get()}
                        <div class="mt-2 space-x-2 ">
                            <button class="btn-primary " on:click=on_import_confirm.clone()>"✅ Importieren (Duplikate überspringen)"</button>
                            <button class="btn-secondary " on:click=move |_| { import_preview.set(String::new()); import_json.set(String::new()); }>"Abbrechen"</button>
                        </div>
                    </div>
                })}
            </section>

            // PED-Card
            <section class="space-y-3 ">
                <h3 class="font-medium ">"💰 PED-Card"</h3>
                <div class="flex gap-2 items-center text-sm ">
                    <label class="flex gap-1 items-center ">
                        "Startkapital (PED):"
                        <input
                            class="input w-28 "
                            type="number"
                            step="0.01"
                            placeholder="0.00"
                            value=move || ped_starting.get()
                            on:input=move |ev| ped_starting.set(event_target_value(&ev))
                        />
                    </label>
                    <button class="btn-secondary text-sm " on:click=move |_| {
                        if let Some(d) = db.get_untracked() {
                            let val = ped_starting.get_untracked();
                            spawn_local(async move {
                                let _ = d.set_setting("ped_starting", &val).await;
                                saved_msg.set("✅ Startkapital gespeichert".into());
                            });
                        }
                    }>"💾"</button>
                </div>
                {move || {
                    let rs = runs_for_ped.get();
                    let starting: f64 = ped_starting.get().parse().unwrap_or(0.0);

                    let total_profit_tt: f64 = rs.iter().map(|r| r.profit_ped).sum();
                    let total_cost:      f64 = rs.iter().map(|r| r.total_cost_ped).sum();
                    let total_loot:      f64 = rs.iter().map(|r| r.stats.total_loot_value_ped).sum();
                    let current_balance = starting + total_profit_tt;
                    let balance_cls = if current_balance >= starting { "text-green-600 font-bold text-xl" } else { "text-red-500 font-bold text-xl" };

                    view! {
                        <div class="bg-white border rounded p-3 space-y-2 text-sm ">
                            <div class="text-center ">
                                <div class=balance_cls>{format!("{:.2} PED", current_balance)}</div>
                                <div class="text-xs text-gray-400 ">"Geschätzte PED-Card"</div>
                            </div>
                            <div class="grid grid-cols-2 gap-2 text-xs ">
                                <div class="text-gray-500 ">"Startkapital:" <span class="float-right font-mono ">{format!("{:.2}", starting)}</span></div>
                                <div class="text-gray-500 ">"Runs:" <span class="float-right font-mono ">{rs.len()}</span></div>
                                <div class="text-gray-500 ">"Gesamtkosten:" <span class="float-right font-mono text-red-500 ">{format!("{:.2}", total_cost)}</span></div>
                                <div class="text-gray-500 ">"Loot TT:" <span class="float-right font-mono ">{format!("{:.2}", total_loot)}</span></div>
                                <div class="col-span-2 text-gray-500 ">"P/L TT:"
                                    <span class={if total_profit_tt >= 0.0 {"float-right font-mono text-green-600"} else {"float-right font-mono text-red-500"}}>
                                        {format!("{:+.2} PED", total_profit_tt)}
                                    </span>
                                </div>
                            </div>
                            <div class="text-xs text-gray-400 italic ">"Basiert auf TT-Werten der gespeicherten Runs. Kein Echtzeit-PED-Kontostand."</div>
                        </div>
                    }
                }}
            </section>

            // Skill-Wert
            <section class="space-y-2 ">
                <h3 class="font-medium ">"🎯 Skill-Wert"</h3>
                <div class="flex gap-2 items-center text-sm ">
                    <label class="flex gap-1 items-center ">
                        "PED pro Skill-Punkt:"
                        <input
                            class="input w-28 "
                            type="number"
                            step="0.01"
                            placeholder="0.00"
                            value=move || ped_per_skillpoint.get()
                            on:input=move |ev| ped_per_skillpoint.set(event_target_value(&ev))
                        />
                    </label>
                    <button class="btn-secondary text-sm " on:click=move |_| {
                        if let Some(d) = db.get_untracked() {
                            let val = ped_per_skillpoint.get_untracked();
                            spawn_local(async move {
                                let _ = d.set_setting("ped_per_skillpoint", &val).await;
                                saved_msg.set("✅ Skill-Wert gespeichert".into());
                            });
                        }
                    }>"💾"</button>
                </div>
                <p class="text-xs text-gray-500 ">"Wird in der Analyse für den PED-Wert der Skill-Gains verwendet."</p>
            </section>

            // Skill-Level-Referenz
            <section class="space-y-2 ">
                <h3 class="font-medium ">"📊 Skill-Level-Referenz (für logarithmische Prognose)"</h3>
                <p class="text-xs text-gray-500 leading-relaxed ">
                    "Aktuellen Skill-Stand eintragen damit die Prognose die Diminishing Returns korrekt berechnet. "
                    "Format: <strong>SkillName=Punkte</strong>, kommasepariert."
                </p>
                <div class="space-y-1 ">
                    <textarea
                        class="input w-full h-24 font-mono text-xs "
                        placeholder="Evader=450,Longblades=320,Paramedic=210"
                        prop:value=move || skill_levels_str.get()
                        on:input=move |ev| skill_levels_str.set(event_target_value(&ev))
                    />
                    <div class="flex items-center gap-3 ">
                        <button class="btn-secondary text-sm " on:click=move |_| {
                            if let Some(d) = db.get_untracked() {
                                let val = skill_levels_str.get_untracked();
                                spawn_local(async move {
                                    let _ = d.set_setting("skill_levels", &val).await;
                                    saved_msg.set("✅ Skill-Level-Referenz gespeichert".into());
                                });
                            }
                        }>"💾 Speichern"</button>
                        <span class="text-xs text-gray-400 ">
                            {move || {
                                let s = skill_levels_str.get();
                                let count = s.split(',').filter(|p| p.contains('=')).count();
                                if count > 0 { format!("{} Skills eingetragen", count) }
                                else { String::new() }
                            }}
                        </span>
                    </div>
                </div>
                <div class="text-xs text-gray-400 space-y-0.5 ">
                    <p>"Skill-Namen findest du in EU unter: Character → Skills (exakter Name)."</p>
                    <p>"Beispiel: <code>Athletics=580,Evader=340,First Aid=120</code>"</p>
                </div>
            </section>

            // Daten zurücksetzen
            <section class="space-y-2 ">
                <h3 class="font-medium text-red-600 ">"Daten zurücksetzen"</h3>
                <button class="bg-red-600 text-white px-3 py-1.5 rounded text-sm hover:bg-red-700 "
                    on:click=on_reset>
                    "🗑 Alle Daten löschen"
                </button>
            </section>
        </div>
    }
}
