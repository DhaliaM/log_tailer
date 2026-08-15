use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::domain::types::{Material, StockEntry, slugify};
use crate::persistence::idb::Db;

#[component]
pub fn StockPanel() -> impl IntoView {
    let (db, set_db)       = signal_local::<Option<Db>>(None);
    let (stock, set_stock) = signal_local::<Vec<StockEntry>>(vec![]);
    let (mats, set_mats)   = signal_local::<Vec<Material>>(vec![]);
    let search             = RwSignal::new(String::new());
    let add_name           = RwSignal::new(String::new());
    let add_qty            = RwSignal::new(String::from("1"));
    let status             = RwSignal::new(String::new());

    let reload = {
        let set_stock = set_stock.clone();
        move || {
            if let Some(d) = db.get_untracked() {
                spawn_local(async move {
                    if let Ok(v) = d.get_all_stock().await {
                        let mut s = v;
                        s.sort_by(|a, b| a.name.cmp(&b.name));
                        set_stock.set(s);
                    }
                });
            }
        }
    };

    Effect::new({
        let set_db = set_db.clone();
        let reload = reload.clone();
        move |_| {
            spawn_local(async move {
                if let Ok(d) = Db::open().await {
                    if let Ok(v) = d.get_all_materials().await { set_mats.set(v); }
                    set_db.set(Some(d));
                    reload();
                }
            });
        }
    });

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let name = add_name.get_untracked().trim().to_string();
            let qty: f64 = add_qty.get_untracked().parse().unwrap_or(0.0);
            if name.is_empty() || qty <= 0.0 { return; }
            if let Some(d) = db.get_untracked() {
                let reload = reload.clone();
                let entry = StockEntry { id: slugify(&name), name, qty };
                spawn_local(async move {
                    let _ = d.save_stock(&entry).await;
                    reload();
                });
                add_name.set(String::new());
                add_qty.set("1".to_string());
                status.set(String::new());
            }
        }
    };

    let mat_names: Vec<String> = {
        let m = mats.get_untracked();
        m.iter().map(|m| m.name.clone()).collect()
    };

    view! {
        <div class="space-y-4 ">
            <h2 class="text-lg font-semibold ">"🏭 Rohstofflager"</h2>
            <p class="text-sm text-gray-500 ">
                "Vorrat an Rohstoffen erfassen. Wird im Craft-Run als Bestandsanzeige genutzt."
            </p>

            // ── Hinzufügen ───────────────────────────────────────────────────
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <label class="flex flex-col gap-1 ">
                    "Material"
                    <input
                        class="input w-48 "
                        list="stock-mat-list"
                        placeholder="z.B. Lysterium Ingot"
                        prop:value=move || add_name.get()
                        on:input=move |ev| add_name.set(event_target_value(&ev))
                    />
                    <datalist id="stock-mat-list">
                        {move || mats.get().into_iter().map(|m| {
                            view!{ <option value=m.name /> }
                        }).collect_view()}
                    </datalist>
                </label>
                <label class="flex flex-col gap-1 ">
                    "Menge"
                    <input
                        class="input w-24 "
                        type="number" min="0" step="1"
                        prop:value=move || add_qty.get()
                        on:input=move |ev| add_qty.set(event_target_value(&ev))
                    />
                </label>
                <button class="btn-primary self-end " on:click=on_add>"+ Einlagern"</button>
                {move || if status.get().is_empty() { view!{<span></span>}.into_any() } else {
                    view!{<span class="text-sm text-green-600 ">{status.get()}</span>}.into_any()
                }}
            </div>

            // ── Suche ────────────────────────────────────────────────────────
            <input class="input w-full " placeholder="Suchen…"
                prop:value=move || search.get()
                on:input=move |ev| search.set(event_target_value(&ev)) />

            // ── Tabelle ──────────────────────────────────────────────────────
            {move || {
                let q = search.get().to_lowercase();
                let entries: Vec<_> = stock.get().into_iter()
                    .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q))
                    .collect();

                if entries.is_empty() {
                    return view!{<p class="text-gray-400 italic text-sm ">
                        {if stock.get().is_empty() { "Lager ist leer." } else { "Keine Treffer." }}
                    </p>}.into_any();
                }

                let total = entries.len();
                view!{
                    <div>
                        <p class="text-xs text-gray-400 mb-1 ">{format!("{total} Einträge")}</p>
                        <table class="w-full text-sm border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-2 ">"Material"</th>
                                    <th class="text-right p-2 ">"Menge"</th>
                                    <th class="p-2 "></th>
                                </tr>
                            </thead>
                            <tbody>
                                {entries.into_iter().map(|entry| {
                                    let qty_sig = RwSignal::new(format!("{}", entry.qty));
                                    let entry_s = StoredValue::new(entry.clone());
                                    let id_del  = entry.id.clone();
                                    let reload2 = reload.clone();
                                    let reload3 = reload.clone();
                                    let reload4 = reload.clone();
                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-2 font-medium ">{entry.name.clone()}</td>
                                            <td class="p-2 text-right ">
                                                <div class="flex items-center justify-end gap-1 ">
                                                    <button class="btn-xs w-6 " on:click=move |_| {
                                                        let e = entry_s.get_value();
                                                        let new_qty = (e.qty - 1.0).max(0.0);
                                                        qty_sig.set(format!("{}", new_qty));
                                                        if let Some(d) = db.get_untracked() {
                                                            let r = reload2.clone();
                                                            spawn_local(async move {
                                                                let _ = d.save_stock(&StockEntry { qty: new_qty, ..e }).await;
                                                                r();
                                                            });
                                                        }
                                                    }>"−"</button>
                                                    <input class="input w-20 text-right "
                                                        type="number" min="0" step="1"
                                                        prop:value=move || qty_sig.get()
                                                        on:change=move |ev| {
                                                            let val: f64 = event_target_value(&ev).parse().unwrap_or(0.0);
                                                            qty_sig.set(format!("{}", val));
                                                            let e = entry_s.get_value();
                                                            if let Some(d) = db.get_untracked() {
                                                                let r = reload3.clone();
                                                                spawn_local(async move {
                                                                    let _ = d.save_stock(&StockEntry { qty: val, ..e }).await;
                                                                    r();
                                                                });
                                                            }
                                                        }
                                                    />
                                                    <button class="btn-xs w-6 " on:click=move |_| {
                                                        let e = entry_s.get_value();
                                                        let new_qty = e.qty + 1.0;
                                                        qty_sig.set(format!("{}", new_qty));
                                                        if let Some(d) = db.get_untracked() {
                                                            let r = reload4.clone();
                                                            spawn_local(async move {
                                                                let _ = d.save_stock(&StockEntry { qty: new_qty, ..e }).await;
                                                                r();
                                                            });
                                                        }
                                                    }>"+"</button>
                                                </div>
                                            </td>
                                            <td class="p-2 text-right ">
                                                <button class="btn-xs-danger " on:click=move |_| {
                                                    if let Some(d) = db.get_untracked() {
                                                        let id = id_del.clone();
                                                        let r = reload.clone();
                                                        spawn_local(async move {
                                                            let _ = d.delete_stock(&id).await;
                                                            r();
                                                        });
                                                    }
                                                }>"🗑"</button>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
