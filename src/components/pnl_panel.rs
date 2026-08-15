use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::*;
use uuid::Uuid;

use crate::domain::calc::{calc_one, CalcParams};
use crate::domain::types::{
    Blueprint, InventoryEntry, Material, PnlSummary, PurchaseRecord, SaleRecord,
};
use crate::persistence::idb::Db;

fn now_iso() -> String {
    let ms = js_sys::Date::now() as u64;
    let secs = ms / 1000;
    // Einfaches ISO-Format: Unix-Timestamp als String (lesbar genug für Log)
    format!("{secs}")
}

#[component]
pub fn PnlPanel() -> impl IntoView {
    let (db, set_db)          = signal_local::<Option<Db>>(None);
    let (blueprints, set_bps) = signal_local::<Vec<Blueprint>>(vec![]);
    let (materials, set_mats) = signal_local::<Vec<Material>>(vec![]);
    let (inventory, set_inv)  = signal_local::<Vec<InventoryEntry>>(vec![]);
    let (sales, set_sales)    = signal_local::<Vec<SaleRecord>>(vec![]);
    let (purchases, set_purs) = signal_local::<Vec<PurchaseRecord>>(vec![]);
    let status_msg = RwSignal::new(String::new());

    // ── Formular-Signals ─────────────────────────────────────────────────────
    let prod_bp_id = RwSignal::new(String::new());
    let prod_qty   = RwSignal::new(String::from("1"));
    let prod_sr    = RwSignal::new(0.95f64);
    let sale_item  = RwSignal::new(String::new());
    let sale_qty   = RwSignal::new(String::from("1"));
    let sale_price = RwSignal::new(String::from("0"));
    let pur_mat    = RwSignal::new(String::new());
    let pur_qty    = RwSignal::new(String::from("1"));
    let pur_markup = RwSignal::new(String::from("100"));

    // ── DB laden ─────────────────────────────────────────────────────────────
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await { set_db.set(Some(d)); }
        });
    });

    let reload = {
        let db = db.clone();
        move || {
            if let Some(d) = db.get() {
                let (d2, d3, d4, d5) = (d.clone(), d.clone(), d.clone(), d.clone());
                spawn_local(async move { if let Ok(v) = d.get_all_blueprints().await  { set_bps.set(v);   }});
                spawn_local(async move { if let Ok(v) = d2.get_all_materials().await  { set_mats.set(v);  }});
                spawn_local(async move { if let Ok(v) = d3.get_all_inventory().await  { set_inv.set(v);   }});
                spawn_local(async move { if let Ok(v) = d4.get_all_sales().await      { set_sales.set(v); }});
                spawn_local(async move { if let Ok(v) = d5.get_all_purchases().await  { set_purs.set(v);  }});
            }
        }
    };
    Effect::new({ let r = reload.clone(); move |_| { let _ = db.get(); r(); } });

    // ── P&L Berechnung ───────────────────────────────────────────────────────
    let pnl = move || {
        let mut s = PnlSummary::default();
        for r in &sales.get()     { s.total_revenue += r.price_ped; s.total_cost += r.cost_basis_ped; }
        for p in &purchases.get() { s.total_purchase_spend += p.total_ped; }
        s
    };

    // ── Aktionen ─────────────────────────────────────────────────────────────

    let record_production = {
        let reload = reload.clone();
        move |_| {
            let bp_id = prod_bp_id.get_untracked();
            if bp_id.is_empty() { status_msg.set("⚠ Blueprint wählen".into()); return; }
            let qty: f64 = prod_qty.get_untracked().parse().unwrap_or(1.0);
            let bps = blueprints.get_untracked();
            let mats = materials.get_untracked();
            let Some(bp) = bps.iter().find(|b| b.id == bp_id).cloned() else {
                status_msg.set("⚠ Blueprint nicht gefunden".into()); return;
            };
            let params = CalcParams { success_rate: prod_sr.get_untracked(), ..Default::default() };
            let unit_cost = calc_one(&bp, &bps, &mats, &params).cost_per_output;
            let product_name = bp.product_name.clone();
            let Some(d) = db.get_untracked() else { return };
            let reload2 = reload.clone();
            spawn_local(async move {
                // Bestehenden Inventar-Eintrag updaten oder neu anlegen
                let mut inv = d.get_all_inventory().await.unwrap_or_default();
                if let Some(e) = inv.iter_mut().find(|e| e.item_id == bp_id) {
                    let total = e.qty + qty;
                    e.avg_cost_ped = (e.avg_cost_ped * e.qty + unit_cost * qty) / total;
                    e.qty = total;
                    let _ = d.save_inventory_entry(e).await;
                } else {
                    let entry = InventoryEntry {
                        item_id:      bp_id.clone(),
                        item_name:    product_name.clone(),
                        qty,
                        avg_cost_ped: unit_cost,
                    };
                    let _ = d.save_inventory_entry(&entry).await;
                }
                status_msg.set(format!("✅ {qty:.0}× {product_name} eingebucht (Ø {unit_cost:.4} PED/Stk)"));
                reload2();
            });
        }
    };

    let record_sale = {
        let reload = reload.clone();
        move |_| {
            let item_id = sale_item.get_untracked();
            if item_id.is_empty() { status_msg.set("⚠ Item wählen".into()); return; }
            let qty: f64   = sale_qty.get_untracked().parse().unwrap_or(1.0);
            let price: f64 = sale_price.get_untracked().parse().unwrap_or(0.0);
            let inv = inventory.get_untracked();
            let entry = inv.iter().find(|e| e.item_id == item_id).cloned();
            let cost_basis  = entry.as_ref().map(|e| e.avg_cost_ped * qty).unwrap_or(0.0);
            let item_name   = entry.as_ref().map(|e| e.item_name.clone()).unwrap_or_else(|| item_id.clone());
            let Some(d) = db.get_untracked() else { return };
            let reload2 = reload.clone();
            spawn_local(async move {
                let sale = SaleRecord {
                    id: Uuid::new_v4().to_string(),
                    item_id: item_id.clone(),
                    item_name: item_name.clone(),
                    qty, price_ped: price, cost_basis_ped: cost_basis,
                    sold_at: now_iso(),
                };
                let _ = d.save_sale(&sale).await;
                // Inventar reduzieren
                let mut inv = d.get_all_inventory().await.unwrap_or_default();
                if let Some(e) = inv.iter_mut().find(|e| e.item_id == item_id) {
                    e.qty = (e.qty - qty).max(0.0);
                    let _ = d.save_inventory_entry(e).await;
                }
                status_msg.set(format!("✅ {qty:.0}× {item_name} für {price:.4} PED verkauft"));
                reload2();
            });
        }
    };

    let record_purchase = {
        let reload = reload.clone();
        move |_| {
            let mat_id = pur_mat.get_untracked();
            if mat_id.is_empty() { status_msg.set("⚠ Material wählen".into()); return; }
            let qty: f64    = pur_qty.get_untracked().parse().unwrap_or(1.0);
            let markup: f64 = pur_markup.get_untracked().parse().unwrap_or(100.0);
            let mats = materials.get_untracked();
            let tt = mats.iter().find(|m| m.id == mat_id).map(|m| m.tt_value).unwrap_or(0.0);
            let total = qty * tt * (markup / 100.0);
            let mat_name = mats.iter().find(|m| m.id == mat_id)
                .map(|m| m.name.clone()).unwrap_or_else(|| mat_id.clone());
            let Some(d) = db.get_untracked() else { return };
            let reload2 = reload.clone();
            spawn_local(async move {
                let pur = PurchaseRecord {
                    id: Uuid::new_v4().to_string(),
                    material_id: mat_id, material_name: mat_name.clone(),
                    qty, tt_paid: tt, markup_paid_pct: markup,
                    total_ped: total, purchased_at: now_iso(),
                };
                let _ = d.save_purchase(&pur).await;
                status_msg.set(format!("✅ {qty:.0}× {mat_name} eingekauft ({total:.4} PED)"));
                reload2();
            });
        }
    };

    view! {
        <div class="space-y-4 max-w-3xl ">
            <h2 class="text-lg font-semibold ">"💰 Inventar & P&L"</h2>

            // ── Status ───────────────────────────────────────────────────────
            {move || { let s = status_msg.get(); if s.is_empty() { return view!{<span></span>}.into_any(); }
                view!{ <div class="text-sm p-2 bg-gray-50 rounded border ">{s}</div> }.into_any()
            }}

            // ── P&L Summary ──────────────────────────────────────────────────
            {move || {
                let p = pnl();
                let profit_cls = if p.profit() >= 0.0 { "font-mono font-semibold text-green-600" }
                                 else                  { "font-mono font-semibold text-red-500" };
                view! {
                    <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 ">
                        <div class="border rounded p-3 bg-white ">
                            <div class="text-xs text-gray-500 ">"Umsatz"</div>
                            <div class="font-mono font-semibold ">{format!("{:.2} PED", p.total_revenue)}</div>
                        </div>
                        <div class="border rounded p-3 bg-white ">
                            <div class="text-xs text-gray-500 ">"Herstellungskosten"</div>
                            <div class="font-mono font-semibold ">{format!("{:.2} PED", p.total_cost)}</div>
                        </div>
                        <div class="border rounded p-3 bg-white ">
                            <div class="text-xs text-gray-500 ">"Gewinn"</div>
                            <div class=profit_cls>
                                {format!("{:+.2} PED ({:+.1}%)", p.profit(), p.roi_pct())}
                            </div>
                        </div>
                        <div class="border rounded p-3 bg-white ">
                            <div class="text-xs text-gray-500 ">"Materialeinkäufe"</div>
                            <div class="font-mono font-semibold ">{format!("{:.2} PED", p.total_purchase_spend)}</div>
                        </div>
                    </div>
                }
            }}

            // ── Formulare ────────────────────────────────────────────────────
            <div class="grid gap-4 sm:grid-cols-3 ">

                // Produktion einbuchen
                <div class="border rounded p-3 bg-white space-y-2 text-sm ">
                    <h4 class="font-semibold ">"📦 Produktion einbuchen"</h4>
                    <label class="flex flex-col gap-1 ">"Blueprint:"
                        <select class="input " on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            prod_bp_id.set(ev.target().unwrap().dyn_into::<web_sys::HtmlSelectElement>().unwrap().value());
                        }>
                            <option value="">"— wählen —"</option>
                            {move || blueprints.get().into_iter().map(|b| {
                                let id = b.id.clone();
                                view!{ <option value=id>{b.product_name.clone()}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="flex flex-col gap-1 ">"Menge:"
                        <input class="input " type="number" step="1" min="1"
                            prop:value=move || prod_qty.get()
                            on:input=move |ev| prod_qty.set(event_target_value(&ev)) />
                    </label>
                    <div class="flex gap-3 ">
                        <label class="flex items-center gap-1 ">
                            <input type="radio" name="psr"
                                prop:checked=move || prod_sr.get() == 0.90
                                on:change=move |_| prod_sr.set(0.90) /> "90%"
                        </label>
                        <label class="flex items-center gap-1 ">
                            <input type="radio" name="psr"
                                prop:checked=move || prod_sr.get() == 0.95
                                on:change=move |_| prod_sr.set(0.95) /> "95%"
                        </label>
                    </div>
                    <button class="btn-primary w-full " on:click=record_production.clone()>
                        "Einbuchen"
                    </button>
                </div>

                // Verkauf erfassen
                <div class="border rounded p-3 bg-white space-y-2 text-sm ">
                    <h4 class="font-semibold ">"💸 Verkauf erfassen"</h4>
                    <label class="flex flex-col gap-1 ">"Item:"
                        <select class="input " on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            sale_item.set(ev.target().unwrap().dyn_into::<web_sys::HtmlSelectElement>().unwrap().value());
                        }>
                            <option value="">"— wählen —"</option>
                            {move || inventory.get().into_iter()
                                .filter(|e| e.qty > 0.0)
                                .map(|e| {
                                    let id = e.item_id.clone();
                                    let label = format!("{} ({:.0}×)", e.item_name, e.qty);
                                    view!{ <option value=id>{label}</option> }
                                }).collect_view()}
                        </select>
                    </label>
                    <label class="flex flex-col gap-1 ">"Menge:"
                        <input class="input " type="number" step="1" min="1"
                            prop:value=move || sale_qty.get()
                            on:input=move |ev| sale_qty.set(event_target_value(&ev)) />
                    </label>
                    <label class="flex flex-col gap-1 ">"Preis (PED):"
                        <input class="input " type="number" step="0.0001" min="0"
                            prop:value=move || sale_price.get()
                            on:input=move |ev| sale_price.set(event_target_value(&ev)) />
                    </label>
                    <button class="btn-primary w-full " on:click=record_sale.clone()>
                        "Speichern"
                    </button>
                </div>

                // Materialeinkauf
                <div class="border rounded p-3 bg-white space-y-2 text-sm ">
                    <h4 class="font-semibold ">"🛒 Materialeinkauf"</h4>
                    <label class="flex flex-col gap-1 ">"Material:"
                        <select class="input " on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            let id = ev.target().unwrap().dyn_into::<web_sys::HtmlSelectElement>().unwrap().value();
                            if let Some(m) = materials.get_untracked().iter().find(|m| m.id == id) {
                                pur_markup.set(format!("{:.1}", m.markup_pct));
                            }
                            pur_mat.set(id);
                        }>
                            <option value="">"— wählen —"</option>
                            {move || materials.get().into_iter().map(|m| {
                                let id = m.id.clone();
                                view!{ <option value=id>{m.name.clone()}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="flex flex-col gap-1 ">"Menge:"
                        <input class="input " type="number" step="1" min="1"
                            prop:value=move || pur_qty.get()
                            on:input=move |ev| pur_qty.set(event_target_value(&ev)) />
                    </label>
                    <label class="flex flex-col gap-1 ">"Markup %:"
                        <input class="input " type="number" step="0.1" min="100"
                            prop:value=move || pur_markup.get()
                            on:input=move |ev| pur_markup.set(event_target_value(&ev)) />
                    </label>
                    <button class="btn-primary w-full " on:click=record_purchase.clone()>
                        "Speichern"
                    </button>
                </div>
            </div>

            // ── Lagerbestand ─────────────────────────────────────────────────
            <div>
                <h3 class="font-semibold text-sm mb-2 ">"Lagerbestand"</h3>
                {move || {
                    let inv = inventory.get();
                    if inv.is_empty() {
                        return view!{<p class="text-gray-400 italic text-sm ">"Noch kein Lagerbestand."</p>}.into_any();
                    }
                    view! {
                        <table class="w-full text-sm border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="p-2 text-left ">"Item"</th>
                                    <th class="p-2 text-right ">"Menge"</th>
                                    <th class="p-2 text-right ">"Ø Kosten/Stk"</th>
                                    <th class="p-2 text-right ">"Gesamt"</th>
                                </tr>
                            </thead>
                            <tbody>
                            {inv.into_iter().map(|e| view! {
                                <tr class="border-b ">
                                    <td class="p-2 ">{e.item_name.clone()}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.0}", e.qty)}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.4}", e.avg_cost_ped)}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.4}", e.qty * e.avg_cost_ped)}</td>
                                </tr>
                            }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </div>

            // ── Verkaufshistorie ─────────────────────────────────────────────
            <details>
                <summary class="cursor-pointer font-semibold text-sm ">
                    "Verkaufshistorie ("
                    {move || sales.get().len()}
                    ")"
                </summary>
                {move || {
                    let s = sales.get();
                    if s.is_empty() {
                        return view!{<p class="text-gray-400 italic text-sm mt-2 ">"Noch keine Verkäufe."</p>}.into_any();
                    }
                    view! {
                        <table class="w-full text-sm border-collapse mt-2 ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="p-2 text-left ">"Item"</th>
                                    <th class="p-2 text-right ">"Qty"</th>
                                    <th class="p-2 text-right ">"Preis"</th>
                                    <th class="p-2 text-right ">"Kosten"</th>
                                    <th class="p-2 text-right ">"Gewinn"</th>
                                </tr>
                            </thead>
                            <tbody>
                            {s.into_iter().rev().map(|r| {
                                let profit = r.profit();
                                let cls = if profit >= 0.0 { "p-2 text-right font-mono text-green-600" }
                                          else             { "p-2 text-right font-mono text-red-500" };
                                view! {
                                    <tr class="border-b ">
                                        <td class="p-2 ">{r.item_name.clone()}</td>
                                        <td class="p-2 text-right font-mono ">{format!("{:.0}", r.qty)}</td>
                                        <td class="p-2 text-right font-mono ">{format!("{:.4}", r.price_ped)}</td>
                                        <td class="p-2 text-right font-mono ">{format!("{:.4}", r.cost_basis_ped)}</td>
                                        <td class=cls>{format!("{:+.4}", profit)}</td>
                                    </tr>
                                }
                            }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </details>

            // ── Einkaufshistorie ─────────────────────────────────────────────
            <details>
                <summary class="cursor-pointer font-semibold text-sm ">
                    "Einkaufshistorie ("
                    {move || purchases.get().len()}
                    ")"
                </summary>
                {move || {
                    let p = purchases.get();
                    if p.is_empty() {
                        return view!{<p class="text-gray-400 italic text-sm mt-2 ">"Noch keine Einkäufe."</p>}.into_any();
                    }
                    view! {
                        <table class="w-full text-sm border-collapse mt-2 ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="p-2 text-left ">"Material"</th>
                                    <th class="p-2 text-right ">"Qty"</th>
                                    <th class="p-2 text-right ">"Markup"</th>
                                    <th class="p-2 text-right ">"Gesamt"</th>
                                </tr>
                            </thead>
                            <tbody>
                            {p.into_iter().rev().map(|r| view! {
                                <tr class="border-b ">
                                    <td class="p-2 ">{r.material_name.clone()}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.0}", r.qty)}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.1}%", r.markup_paid_pct)}</td>
                                    <td class="p-2 text-right font-mono ">{format!("{:.4}", r.total_ped)}</td>
                                </tr>
                            }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </details>
        </div>
    }
}
