use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::domain::types::{CreatureConfig, Maturity};
use crate::persistence::idb::Db;
use crate::services::nexus_import;

#[component]
pub fn CreaturesPanel() -> impl IntoView {
    let (db, set_db) = signal_local::<Option<Db>>(None);
    let (creatures, set_creatures) = signal_local::<Vec<CreatureConfig>>(vec![]);
    let show_form = RwSignal::new(false);
    let editing = RwSignal::new(Option::<CreatureConfig>::None);

    // DB öffnen
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await {
                set_db.set(Some(d));
            }
        });
    });

    // Laden wenn DB bereit
    let reload = {
        let db = db.clone();
        let set_creatures = set_creatures.clone();
        move || {
            if let Some(d) = db.get() {
                let set_creatures = set_creatures.clone();
                spawn_local(async move {
                    if let Ok(cs) = d.get_all_creatures().await {
                        let mut sorted = cs;
                        sorted.sort_by(|a, b| a.creature.cmp(&b.creature));
                        set_creatures.set(sorted);
                    }
                });
            }
        }
    };

    Effect::new({
        let reload = reload.clone();
        move |_| { let _ = db.get(); reload(); }
    });

    let on_delete = {
        let db = db.clone();
        let reload = reload.clone();
        move |name: String| {
            if !web_sys::window()
                .and_then(|w| w.confirm_with_message(&format!("Kreatur \"{name}\" loeschen?")).ok())
                .unwrap_or(false) { return; }
            if let Some(d) = db.get_untracked() {
                let reload = reload.clone();
                spawn_local(async move {
                    let _ = d.delete_creature(&name).await;
                    reload();
                });
            }
        }
    };

    let on_edit = move |c: CreatureConfig| {
        editing.set(Some(c));
        show_form.set(true);
    };

    let on_new = move |_| {
        editing.set(None);
        show_form.set(true);
    };

    // ── API-Import ────────────────────────────────────────────────────────────
    let show_import    = RwSignal::new(false);
    let import_name    = RwSignal::new(String::new());
    let import_loading = RwSignal::new(false);
    let import_error   = RwSignal::new(String::new());

    let do_import = move || {
        let name = import_name.get_untracked();
        if name.is_empty() { return; }
        import_loading.set(true);
        import_error.set(String::new());
        spawn_local(async move {
            match nexus_import::fetch_creature(&name).await {
                Ok(config) => {
                    import_loading.set(false);
                    show_import.set(false);
                    import_name.set(String::new());
                    editing.set(Some(config));
                    show_form.set(true);
                }
                Err(e) => {
                    import_loading.set(false);
                    import_error.set(e);
                }
            }
        });
    };

    view! {
        <div class="space-y-4 max-w-3xl ">
            <div class="flex items-center justify-between ">
                <h2 class="text-lg font-semibold ">"🐾 Kreaturen "</h2>
                <div class="flex gap-2 ">
                    <button class="btn-secondary "
                        on:click=move |_| { show_import.update(|v| *v = !*v); import_error.set(String::new()); }>
                        "🌐 API-Import"
                    </button>
                    <button class="btn-primary " on:click=on_new>"+ Neue Kreatur "</button>
                </div>
            </div>

            // API-Import-Formular
            <Show when=move || show_import.get()>
                <div class="border rounded p-3 bg-blue-50 border-blue-200 space-y-2 text-sm ">
                    <div class="font-medium text-blue-800 ">"🌐 Von Entropianexus importieren"</div>
                    <div class="text-xs text-blue-600 ">
                        "Mob-Namen eingeben (z.B. \"Wombana\", \"Atrox\") – Daten werden von entropianexus.com geladen."
                    </div>
                    <div class="flex gap-2 items-center ">
                        <input
                            class="input flex-1 "
                            placeholder="Mob-Name..."
                            prop:value=move || import_name.get()
                            on:input=move |ev| import_name.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" { do_import(); }
                            }
                        />
                        <button
                            class="btn-primary "
                            disabled=move || import_loading.get()
                            on:click=move |_| do_import()
                        >
                            {move || if import_loading.get() { "⏳ Laden..." } else { "Importieren" }}
                        </button>
                        <button class="btn-xs " on:click=move |_| show_import.set(false)>"✖"</button>
                    </div>
                    {move || (!import_error.get().is_empty()).then(|| view!{
                        <p class="text-red-600 text-xs ">{move || import_error.get()}</p>
                    })}
                </div>
            </Show>

            // Formular
            <Show when=move || show_form.get()>
                <CreatureForm
                    initial=editing
                    db=move || db.get()
                    on_done=move || { show_form.set(false); reload(); }
                />
            </Show>

            // Liste
            {move || {
                let cs = creatures.get();
                if cs.is_empty() {
                    view!{<p class="text-gray-400 italic ">"Noch keine Kreaturen konfiguriert."</p>}.into_any()
                } else {
                    view! {
                        <table class="w-full text-sm border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-2 ">"Name"</th>
                                    <th class="text-left p-2 ">"Maturities"</th>
                                    <th class="p-2 "></th>
                                </tr>
                            </thead>
                            <tbody>
                                {cs.into_iter().map(|c| {
                                    let name = c.creature.clone();
                                    let mats = c.maturities.iter().map(|m| m.name.clone()).collect::<Vec<_>>().join(", ");
                                    let c_edit = c.clone();
                                    let name_del = name.clone();
                                    let on_delete = on_delete.clone();
                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-2 font-medium ">{name.clone()}</td>
                                            <td class="p-2 text-gray-600 ">{mats}</td>
                                            <td class="p-2 space-x-2 text-right ">
                                                <button class="btn-xs " on:click=move |_| on_edit(c_edit.clone())>"✏"</button>
                                                <button class="btn-xs-danger " on:click=move |_| on_delete(name_del.clone())>"🗑"</button>
                                            </td>
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

// ─── CreatureForm ─────────────────────────────────────────────────────────────

#[component]
fn CreatureForm<F: Fn() + 'static + Clone>(
    initial: RwSignal<Option<CreatureConfig>>,
    db: impl Fn() -> Option<Db> + 'static + Clone + Copy,
    on_done: F,
) -> impl IntoView {
    let creature_name = RwSignal::new(String::new());
    let maturities = RwSignal::new(vec![
        Maturity { name: String::new(), hp_min: 0.0, hp_max: 0.0 }
    ]);
    let error = RwSignal::new(String::new());

    // Initialisierung wenn editing gesetzt
    Effect::new(move |_| {
        if let Some(c) = initial.get() {
            creature_name.set(c.creature.clone());
            maturities.set(c.maturities.clone());
        }
    });

    let add_maturity = move |_| {
        maturities.update(|v| v.push(Maturity { name: String::new(), hp_min: 0.0, hp_max: 0.0 }));
    };

    let on_save = {
        let on_done = on_done.clone();
        move |_| {
            let name = creature_name.get_untracked();
            let mats = maturities.get_untracked();
            if name.is_empty() {
                error.set("Name darf nicht leer sein.".into());
                return;
            }
            if mats.is_empty() {
                error.set("Mindestens eine Maturity erforderlich.".into());
                return;
            }
            for m in &mats {
                if m.hp_min <= 0.0 {
                    error.set(format!("HP muss > 0 sein ({})", m.name));
                    return;
                }
            }
            error.set(String::new());
            let config = CreatureConfig { creature: name, maturities: mats };
            let on_done = on_done.clone();
            if let Some(d) = db() {
                spawn_local(async move {
                    let _ = d.save_creature(&config).await;
                    on_done();
                });
            }
        }
    };

    let on_cancel = { let f = on_done.clone(); move |_| f() };

    view! {
        <div class="border rounded p-4 bg-white space-y-3 ">
            <h3 class="font-medium ">{move || if initial.get().is_some() { "Kreatur bearbeiten " } else { "Neue Kreatur " }}</h3>
            {move || (!error.get().is_empty()).then(|| view!{
                <p class="text-red-500 text-sm ">{move || error.get()}</p>
            })}
            <label class="flex flex-col gap-1 text-sm ">
                "Kreatur-Name "
                <input class="input " value=move || creature_name.get()
                    on:input=move |ev| creature_name.set(event_target_value(&ev)) />
            </label>
            <div class="space-y-2 ">
                <div class="flex items-center justify-between ">
                    <span class="text-sm font-medium ">"Maturities"</span>
                    <button class="btn-xs " on:click=add_maturity>"+ Hinzufuegen "</button>
                </div>
                {move || {
                    maturities.get().into_iter().enumerate().map(|(i, _)| {
                        view! {
                            <div class="flex gap-1 items-center text-sm ">
                                // Reihenfolge-Buttons
                                <div class="flex flex-col gap-0.5 ">
                                    <button class="btn-xs px-1 py-0 leading-none "
                                        disabled=move || i == 0
                                        on:click=move |_| maturities.update(|v| { if i > 0 { v.swap(i - 1, i); } })>
                                        "▲"
                                    </button>
                                    <button class="btn-xs px-1 py-0 leading-none "
                                        disabled=move || i + 1 >= maturities.get().len()
                                        on:click=move |_| maturities.update(|v| { if i + 1 < v.len() { v.swap(i, i + 1); } })>
                                        "▼"
                                    </button>
                                </div>
                                <input class="input flex-1 " placeholder="Name (z.B. Young)"
                                    prop:value=move || maturities.get().get(i).map(|m| m.name.clone()).unwrap_or_default()
                                    on:input=move |ev| maturities.update(|v| {
                                        if let Some(entry) = v.get_mut(i) { entry.name = event_target_value(&ev); }
                                    }) />
                                <input class="input w-16 " type="number" step="0.1" placeholder="HP"
                                    prop:value=move || maturities.get().get(i)
                                        .map(|m| if m.hp_min > 0.0 { m.hp_min.to_string() } else { String::new() })
                                        .unwrap_or_default()
                                    on:input=move |ev| maturities.update(|v| {
                                        if let Some(entry) = v.get_mut(i) {
                                            let hp = event_target_value(&ev).parse().unwrap_or(0.0);
                                            entry.hp_min = hp;
                                            entry.hp_max = hp;
                                        }
                                    }) />
                                <button class="btn-xs-danger " on:click=move |_| maturities.update(|v| { v.remove(i); })>"✕"</button>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
            <div class="space-x-2 ">
                <button class="btn-primary " on:click=on_save>"Speichern "</button>
                <button class="btn-xs " on:click=on_cancel>"Abbrechen "</button>
            </div>
        </div>
    }
}
