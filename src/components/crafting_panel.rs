use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::*;
use uuid::Uuid;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::domain::calc::{calc_all, calc_one, CalcParams, CalcResult};
use crate::domain::types::{
    Blueprint, CraftRunStats, HuntNeed, HuntPlan, Ingredient, IngredientSource,
    LootItemConfig, Material, SavedRun, SubMode,
};
use crate::persistence::idb::Db;
use crate::services::craft_tailer::start_craft_polling;
use crate::services::drop_index::build_drop_index;
use crate::services::fs_access;
use crate::services::nexus_blueprints::{self, KNOWN_BOOKS};

#[component]
pub fn CraftingPanel() -> impl IntoView {
    let (db, set_db)               = signal_local::<Option<Db>>(None);
    let (blueprints, set_blueprints) = signal_local::<Vec<Blueprint>>(vec![]);
    let (runs, set_runs)           = signal_local::<Vec<SavedRun>>(vec![]);
    let (materials, set_materials) = signal_local::<Vec<Material>>(vec![]);
    let (active_plan, set_active_plan) = signal_local::<Option<HuntPlan>>(None);
    let (loot_configs, set_loot_configs) = signal_local::<Vec<LootItemConfig>>(vec![]);
    let show_form      = RwSignal::new(false);
    let show_import    = RwSignal::new(false);
    let show_calc      = RwSignal::new(false);
    let show_craft_run = RwSignal::new(false);
    let selected_bp    = RwSignal::new(Option::<Blueprint>::None);
    let active_book_tab = RwSignal::new("recent".to_string());
    let import_book   = RwSignal::new(KNOWN_BOOKS[0].1.to_string());
    let import_status = RwSignal::new(String::new());
    let importing     = RwSignal::new(false);
    // Calc-Parameter + Selektion für Rechner-Ansicht
    let success_rate  = RwSignal::new(0.95f64);
    let margin_pct    = RwSignal::new(0.0f64);
    let calc_selected = RwSignal::new(Option::<String>::None);

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
                spawn_local(async move { if let Ok(v) = d.get_all_blueprints().await  { set_blueprints.set(v); }});
                spawn_local(async move { if let Ok(v) = d2.get_all_runs().await       { set_runs.set(v); }});
                spawn_local(async move { if let Ok(v) = d3.get_all_materials().await  { set_materials.set(v); }});
                spawn_local(async move {
                    if let Ok(p) = d4.get_active_hunt_plan().await { set_active_plan.set(p); }
                });
                spawn_local(async move {
                    if let Ok(v) = d5.get_all_loot_item_configs().await { set_loot_configs.set(v); }
                });
            }
        }
    };

    Effect::new({ let r = reload.clone(); move |_| { let _ = db.get(); r(); } });

    view! {
        <div class="space-y-4 max-w-3xl ">
            <div class="flex items-center justify-between ">
                <h2 class="text-lg font-semibold ">"🔨 Crafting"</h2>
                <div class="flex gap-2 flex-wrap ">
                    <button class="btn-secondary "
                        on:click=move |_| { show_import.update(|v| *v = !*v); show_form.set(false); show_calc.set(false); }>
                        "📥 Nexus Import"
                    </button>
                    <button class=move || if show_calc.get() { "btn-primary" } else { "btn-secondary" }
                        on:click=move |_| { show_calc.update(|v| *v = !*v); show_form.set(false); show_import.set(false); show_craft_run.set(false); }>
                        "🧮 Rechner"
                    </button>
                    <button class=move || if show_craft_run.get() { "btn-primary" } else { "btn-secondary" }
                        on:click=move |_| { show_craft_run.update(|v| *v = !*v); show_form.set(false); show_import.set(false); show_calc.set(false); }>
                        "🔨 Craft-Run"
                    </button>
                    <button class="btn-primary "
                        on:click=move |_| { selected_bp.set(None); show_form.set(true); show_import.set(false); show_calc.set(false); show_craft_run.set(false); }>
                        "+ Manuell"
                    </button>
                </div>
            </div>

            // ── Nexus Import Dialog ──────────────────────────────────────────
            <Show when=move || show_import.get()>
                <NexusImportDialog
                    db=move || db.get()
                    materials=materials
                    blueprints=blueprints
                    import_book=import_book
                    import_status=import_status
                    importing=importing
                    on_done=move || { show_import.set(false); reload(); }
                />
            </Show>

            // ── Craft-Run-Sektion ────────────────────────────────────────────
            <Show when=move || show_craft_run.get()>
                <CraftRunSection />
            </Show>

            // Aktiver Hunt-Plan Banner
            {move || {
                let plan = active_plan.get();
                if let Some(p) = plan {
                    let db2     = db.clone();
                    let reload2 = reload.clone();
                    view! {
                        <div class="bg-green-50 border border-green-300 rounded p-3 flex justify-between items-center text-sm ">
                            <div>
                                <span class="font-semibold text-green-800 ">"🎯 Aktives Hunt-Ziel: "</span>
                                <span class="text-green-700 ">{p.blueprint_name} " × " {p.craft_cycles} " Zyklen"</span>
                                <div class="text-xs text-green-600 mt-1 ">
                                    {p.needs.iter().map(|n| format!(
                                        "{}: {:.0} benötigt ({:.0} vorhanden)",
                                        n.item, n.total_needed, n.already_have
                                    )).collect::<Vec<_>>().join(" · ")}
                                </div>
                            </div>
                            <button class="btn-xs-danger " on:click=move |_| {
                                if let Some(d) = db2.get_untracked() {
                                    let reload = reload2.clone();
                                    spawn_local(async move {
                                        let _ = d.clear_active_hunt_plan().await;
                                        reload();
                                    });
                                }
                            }>"✖ Deaktivieren"</button>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            <Show when=move || show_form.get()>
                <BlueprintForm
                    initial=selected_bp
                    materials=materials.get()
                    db=move || db.get()
                    on_done=move || { show_form.set(false); reload(); }
                />
            </Show>

            // ── Rechner-Ansicht ──────────────────────────────────────────────
            // Steuerung immer sichtbar wenn show_calc aktiv
            <Show when=move || show_calc.get()>
                <div class="flex flex-wrap gap-4 items-center p-3 bg-gray-50 rounded border text-sm ">
                    <label class="flex items-center gap-2 ">
                        "Erfolgsrate:"
                        <label class="flex items-center gap-1 ">
                            <input type="radio" name="calc-sr"
                                prop:checked=move || success_rate.get() == 0.90
                                on:change=move |_| success_rate.set(0.90) />
                            "90%"
                        </label>
                        <label class="flex items-center gap-1 ">
                            <input type="radio" name="calc-sr"
                                prop:checked=move || success_rate.get() == 0.95
                                on:change=move |_| success_rate.set(0.95) />
                            "95%"
                        </label>
                    </label>
                    <label class="flex items-center gap-2 ">
                        "Gewinnmarge %:"
                        <input class="input w-20 text-right " type="number" step="0.5"
                            prop:value=move || format!("{:.1}", margin_pct.get())
                            on:change=move |ev| {
                                use wasm_bindgen::JsCast;
                                let el = ev.target().unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
                                if let Ok(v) = el.value().parse::<f64>() { margin_pct.set(v); }
                            } />
                    </label>
                </div>
            </Show>

            // Rechner Tabelle + Breakdown (reactive über move ||)
            {move || {
                if !show_calc.get() { return view!{<div></div>}.into_any(); }
                let bps  = blueprints.get();
                let mats = materials.get();
                if bps.is_empty() {
                    return view!{<p class="text-gray-400 italic ">"Keine Blueprints vorhanden."</p>}.into_any();
                }
                let params = CalcParams {
                    success_rate: success_rate.get(),
                    desired_margin_pct: margin_pct.get(),
                    ..Default::default()
                };
                let mut results = calc_all(&bps, &mats, &params);
                results.sort_by(|a, b| b.profit_price.partial_cmp(&a.profit_price).unwrap_or(std::cmp::Ordering::Equal));
                let sel = calc_selected.get();
                let detail: Option<CalcResult> = sel.as_ref().and_then(|id| results.iter().find(|r| &r.bp_id == id).cloned());

                view! {
                    <div class="space-y-3 ">
                        <div class="text-xs text-gray-400 ">{results.len()} " Blueprints · Sortiert nach Profit"</div>
                        <table class="w-full text-sm border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="p-2 text-left ">"Blueprint"</th>
                                    <th class="p-2 text-right ">"Kosten"</th>
                                    <th class="p-2 text-right ">"Verkaufspreis"</th>
                                    <th class="p-2 text-right ">"TT Output"</th>
                                </tr>
                            </thead>
                            <tbody>
                            {results.into_iter().map(|r| {
                                let id         = r.bp_id.clone();
                                let profit     = r.profit_price - r.cost_per_output;
                                let profit_cls = if profit >= 0.0 { "p-2 text-right font-mono text-green-600" }
                                                 else             { "p-2 text-right font-mono text-red-500" };
                                let profit_sign = if profit >= 0.0 { " ✅" } else { " ❌" };
                                let is_sel     = sel.as_deref() == Some(&r.bp_id);
                                let row_cls    = if is_sel { "border-b bg-blue-50 cursor-pointer" }
                                                 else      { "border-b hover:bg-gray-50 cursor-pointer" };
                                view! {
                                    <tr class=row_cls on:click=move |_| {
                                        let cur = calc_selected.get_untracked();
                                        if cur.as_deref() == Some(&id) { calc_selected.set(None); }
                                        else { calc_selected.set(Some(id.clone())); }
                                    }>
                                        <td class="p-2 font-medium ">{r.product_name.clone()}</td>
                                        <td class="p-2 text-right font-mono ">{format!("{:.4}", r.cost_per_output)}</td>
                                        <td class=profit_cls>
                                            {format!("{:.4}", r.profit_price)} {profit_sign}
                                        </td>
                                        <td class="p-2 text-right font-mono text-gray-400 ">{format!("{:.4}", r.output_tt_value)}</td>
                                    </tr>
                                }
                            }).collect_view()}
                            </tbody>
                        </table>
                        {detail.map(|d| view! {
                            <div class="border rounded p-4 bg-white space-y-2 ">
                                <h3 class="font-semibold ">"🔍 " {d.product_name.clone()} " – Zutaten"</h3>
                                <table class="w-full text-sm border-collapse ">
                                    <thead class="border-b bg-gray-50 ">
                                        <tr>
                                            <th class="p-2 text-left ">"Zutat"</th>
                                            <th class="p-2 text-right ">"Menge"</th>
                                            <th class="p-2 text-right ">"Preis/Stk"</th>
                                            <th class="p-2 text-right ">"Gesamt"</th>
                                            <th class="p-2 text-left text-xs ">"Typ"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                    {d.breakdown.into_iter().map(|line| view! {
                                        <tr class="border-b ">
                                            <td class="p-2 ">{line.label}</td>
                                            <td class="p-2 text-right font-mono ">{format!("{:.0}", line.qty)}</td>
                                            <td class="p-2 text-right font-mono ">{format!("{:.4}", line.unit_cost)}</td>
                                            <td class="p-2 text-right font-mono ">{format!("{:.4}", line.total_cost)}</td>
                                            <td class="p-2 text-xs text-gray-500 ">
                                                {if line.is_sub_blueprint {"Component"} else {"Material"}}
                                            </td>
                                        </tr>
                                    }).collect_view()}
                                    </tbody>
                                </table>
                                <div class="flex gap-6 text-sm pt-2 border-t ">
                                    <span>"Kosten: "<strong class="font-mono ">{format!("{:.4} PED", d.cost_per_output)}</strong></span>
                                    <span>"Zielpreis: "<strong class="font-mono ">{format!("{:.4} PED", d.profit_price)}</strong></span>
                                </div>
                            </div>
                        })}
                    </div>
                }.into_any()
            }}

            // ── Blueprint-Liste mit Book-Tabs ─────────────────────────────────
            <Show when=move || !show_calc.get()>
            {move || {
                let bps          = blueprints.get();
                let rs           = runs.get();
                let mats         = materials.get();
                let drop_idx     = build_drop_index(&rs);
                let active_id    = active_plan.get().map(|p| p.blueprint_id);
                let loot_cfg_snap = loot_configs.get();
                if bps.is_empty() {
                    return view! { <p class="text-gray-400 italic ">"Noch keine Blueprints."</p> }.into_any();
                }

                // Letzte 10 (nach created_at_sec desc)
                let mut recent = bps.clone();
                recent.retain(|b| b.created_at_sec > 0);
                recent.sort_by(|a, b| b.created_at_sec.cmp(&a.created_at_sec));
                recent.truncate(10);

                // Bekannte Books aus den aktuellen Blueprints (Reihenfolge: KNOWN_BOOKS)
                let mut books: Vec<String> = KNOWN_BOOKS.iter()
                    .map(|(_, key)| key.to_string())
                    .filter(|k| bps.iter().any(|b| b.book.as_deref() == Some(k.as_str())))
                    .collect();
                // Manuelle Blueprints (kein Buch) als eigener Tab
                let has_manual = bps.iter().any(|b| b.book.is_none());
                if has_manual { books.push("(Manuell)".to_string()); }

                let cur_tab = active_book_tab.get();
                let all_bps_snap = bps.clone();

                let tab_bps: Vec<Blueprint> = if cur_tab == "recent" {
                    recent.clone()
                } else if cur_tab == "(Manuell)" {
                    bps.iter().filter(|b| b.book.is_none()).cloned().collect()
                } else {
                    bps.iter().filter(|b| b.book.as_deref() == Some(cur_tab.as_str())).cloned().collect()
                };

                view! {
                    <div class="space-y-2 ">
                        // Tab-Bar
                        <div class="flex flex-wrap gap-1 border-b pb-1 ">
                            {
                                let mut tabs: Vec<(String, String)> = vec![
                                    ("recent".to_string(), format!("🕐 Letzte ({})", recent.len())),
                                ];
                                for b in &books {
                                    let label = KNOWN_BOOKS.iter()
                                        .find(|(_, k)| *k == b.as_str())
                                        .map(|(label, _)| label.to_string())
                                        .unwrap_or_else(|| b.clone());
                                    let count = if b == "(Manuell)" {
                                        bps.iter().filter(|bp| bp.book.is_none()).count()
                                    } else {
                                        bps.iter().filter(|bp| bp.book.as_deref() == Some(b.as_str())).count()
                                    };
                                    tabs.push((b.clone(), format!("{label} ({count})")));
                                }
                                tabs.into_iter().map(|(key, label)| {
                                    let k2 = key.clone();
                                    let is_active = move || active_book_tab.get() == key;
                                    view! {
                                        <button
                                            class=move || if is_active() {
                                                "px-2 py-1 text-xs rounded font-medium bg-blue-600 text-white "
                                            } else {
                                                "px-2 py-1 text-xs rounded font-medium bg-gray-100 hover:bg-gray-200 text-gray-700 "
                                            }
                                            on:click=move |_| active_book_tab.set(k2.clone())
                                        >{label}</button>
                                    }
                                }).collect_view()
                            }
                        </div>
                        // Blueprint-Karten für aktiven Tab
                        <div class="space-y-3 ">
                            {if tab_bps.is_empty() {
                                view!{<p class="text-gray-400 italic text-sm ">"Keine Blueprints in diesem Tab."</p>}.into_any()
                            } else {
                                tab_bps.into_iter().map(|bp| {
                                    let bp2       = bp.clone();
                                    let drop_idx2 = drop_idx.clone();
                                    let loot_cfg2 = loot_cfg_snap.clone();
                                    let all_bps2  = all_bps_snap.clone();
                                    let mats2     = mats.clone();
                                    let is_active = active_id.as_deref() == Some(&bp.id);
                                    view! { <BlueprintCard
                                        bp=bp2
                                        materials=mats2
                                        drop_idx=drop_idx2
                                        loot_configs=loot_cfg2
                                        all_blueprints=all_bps2
                                        is_active=is_active
                                        db=move || db.get()
                                        reload=reload.clone()
                                        on_edit=move |b: Blueprint| { selected_bp.set(Some(b)); show_form.set(true); }
                                        on_activated=move || { reload(); }
                                    /> }
                                }).collect_view().into_any()
                            }}
                        </div>
                    </div>
                }.into_any()
            }}
            </Show>
        </div>
    }
}

// ─── Leaf-Ingredient-Auflösung ────────────────────────────────────────────────

/// Gibt alle Blatt-Material-IDs eines Blueprints zurück (sub-BPs werden expandiert).
fn resolve_leaf_material_ids(bp: &Blueprint, all_blueprints: &[Blueprint], depth: u8) -> Vec<String> {
    if depth == 0 { return vec![]; }
    let mut leaves = Vec::new();
    for ing in &bp.ingredients {
        match &ing.source {
            IngredientSource::Material { material_id } => leaves.push(material_id.clone()),
            IngredientSource::Blueprint { blueprint_id } => {
                if let Some(sub) = all_blueprints.iter().find(|b| &b.id == blueprint_id) {
                    leaves.extend(resolve_leaf_material_ids(sub, all_blueprints, depth - 1));
                } else {
                    leaves.push(blueprint_id.clone());
                }
            }
        }
    }
    leaves.sort();
    leaves.dedup();
    leaves
}

// ─── BlueprintCard ────────────────────────────────────────────────────────────

#[component]
fn BlueprintCard<R, E, A>(
    bp: Blueprint,
    materials: Vec<Material>,
    drop_idx: crate::domain::types::DropIndex,
    #[prop(default = vec![])] loot_configs: Vec<LootItemConfig>,
    #[prop(default = vec![])] all_blueprints: Vec<Blueprint>,
    is_active: bool,
    db: impl Fn() -> Option<Db> + 'static + Copy,
    reload: R,
    on_edit: E,
    on_activated: A,
) -> impl IntoView
where
    R: Fn() + 'static + Clone,
    E: Fn(Blueprint) + 'static + Clone,
    A: Fn() + 'static + Clone,
{
    // Material-ID → Name Lookup
    let mat_name: std::collections::HashMap<&str, &str> = materials.iter()
        .map(|m| (m.id.as_str(), m.name.as_str()))
        .collect();

    // Kosten via Calc-Engine
    let params     = CalcParams::default();
    let calc       = calc_one(&bp, &all_blueprints, &materials, &params);
    let input_cost = calc.cost_per_output;
    let output_tt  = bp.output_tt_value * bp.output_qty;
    let output_mu  = output_tt * (bp.markup_pct / 100.0);
    let profit     = output_mu - input_cost;

    // Blatt-Material-IDs → Display-Namen für Creature-Scoring
    let leaf_ids   = resolve_leaf_material_ids(&bp, &all_blueprints, 3);
    let leaf_names: Vec<&str> = leaf_ids.iter()
        .filter_map(|id| mat_name.get(id.as_str()).copied())
        .collect();

    // Kreatur-Score
    let mut creature_scores: std::collections::HashMap<String, (usize, f64)> = Default::default();

    for (creature, items) in &drop_idx {
        let matches: Vec<_> = leaf_names.iter().filter(|&&ln| items.contains_key(ln)).collect();
        if !matches.is_empty() {
            let avg_drop = matches.iter()
                .map(|&&ln| items.get(ln).map(|d| d.drop_rate).unwrap_or(0.0))
                .sum::<f64>() / matches.len() as f64;
            let entry = creature_scores.entry(creature.clone()).or_insert((0, 0.0));
            if matches.len() > entry.0 { *entry = (matches.len(), avg_drop); }
        }
    }
    for cfg in &loot_configs {
        if leaf_names.contains(&cfg.name.as_str()) {
            for dropper in &cfg.droppers {
                creature_scores.entry(dropper.clone()).or_insert((0, 0.0)).0 += 1;
            }
        }
    }

    let mut recommendations: Vec<(String, usize, f64)> = creature_scores
        .into_iter().map(|(c, (cnt, r))| (c, cnt, r)).collect();
    recommendations.sort_by(|a, b| b.1.cmp(&a.1)
        .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)));
    let total_needed = leaf_ids.len();

    let show_hunt    = RwSignal::new(false);
    let cycles_input = RwSignal::new(String::from("1"));
    let inv_signals: Vec<RwSignal<String>> = bp.ingredients.iter()
        .map(|_| RwSignal::new(String::from("0")))
        .collect();
    let inv_stored   = StoredValue::new(inv_signals.clone());
    let bp_id        = bp.id.clone();
    let bp_edit      = bp.clone();
    let bp_for_hunt  = StoredValue::new(bp.clone());
    let mat_name_stored = StoredValue::new(
        materials.iter().map(|m| (m.id.clone(), m.name.clone())).collect::<std::collections::HashMap<_,_>>()
    );

    view! {
        <div class="border rounded p-3 space-y-2 text-sm bg-white ">
            <div class="flex justify-between items-start ">
                <div>
                    <div class="flex items-center gap-2 ">
                        <span class="font-semibold ">{bp.name.clone()}</span>
                        {if is_active {
                            view! { <span class="text-xs bg-green-100 text-green-700 px-2 py-0.5 rounded-full ">"🎯 Aktiv"</span> }.into_any()
                        } else { view! { <span></span> }.into_any() }}
                    </div>
                    <div class="text-xs text-gray-500 ">
                        "Output: " {bp.product_name.clone()}
                        " ×" {bp.output_qty}
                        " @ " {format!("{:.2} TT", output_tt)}
                    </div>
                </div>
                <div class="flex gap-1 ">
                    <button class="btn-xs " title="Hunt-Ziel setzen"
                        on:click=move |_| show_hunt.update(|v| *v = !*v)>"🎯"</button>
                    <button class="btn-xs " on:click=move |_| on_edit(bp_edit.clone())>"✏"</button>
                    <button class="btn-xs-danger " on:click=move |_| {
                        if let Some(d) = db() {
                            let id     = bp_id.clone();
                            let reload = reload.clone();
                            spawn_local(async move { let _ = d.delete_blueprint(&id).await; reload(); });
                        }
                    }>"🗑"</button>
                </div>
            </div>

            <div class="grid grid-cols-2 gap-1 text-xs bg-gray-50 p-2 rounded ">
                <span>"Herstellungskosten:"</span>
                <span class="font-mono ">{format!("{:.4} PED", input_cost)}</span>
                <span>"Output (TT):"</span>
                <span class="font-mono ">{format!("{:.4} PED", output_tt)}</span>
                <span>"Output (+MU%):"</span>
                <span class="font-mono ">{format!("{:.4} PED", output_mu)}</span>
                <span>"Gewinn/Verlust:"</span>
                <span class={if profit >= 0.0 {"font-mono text-green-600"} else {"font-mono text-red-500"}}>
                    {format!("{:+.4} PED", profit)} {if profit >= 0.0 {" ✅"} else {" ❌"}}
                </span>
            </div>

            {if recommendations.is_empty() { None } else {
                Some(view! {
                    <div class="text-xs space-y-1 ">
                        <div class="font-semibold ">"Kreatur-Empfehlung:"</div>
                        {recommendations.into_iter().take(3).map(|(c, cnt, rate)| view! {
                            <div class="text-gray-700 ">
                                {" ⭐".repeat(cnt.min(3))} " " {c}
                                " – " {cnt} "/" {total_needed} " Zutaten"
                                <span class="text-gray-400 ml-1 ">
                                    {format!("(Ø {:.0}% Drop-Rate)", rate * 100.0)}
                                </span>
                            </div>
                        }).collect_view()}
                    </div>
                })
            }}

            <div style=move || if show_hunt.get() { "" } else { "display:none" }>
                <div class="border-t pt-3 mt-2 space-y-3 ">
                    <div class="font-medium text-sm ">"🎯 Hunt-Ziel konfigurieren"</div>
                    <label class="flex items-center gap-2 text-sm ">
                        "Craft-Zyklen:"
                        <input class="input w-24 " type="number" min="1" step="1"
                            value=move || cycles_input.get()
                            on:input=move |ev| cycles_input.set(event_target_value(&ev)) />
                    </label>
                    <div class="space-y-2 ">
                        <div class="text-xs text-gray-500 font-medium ">"Bereits im Inventar:"</div>
                        {inv_signals.iter().enumerate()
                            .zip(bp_for_hunt.get_value().ingredients.iter())
                            .map(|((_i, &inv), ing)| {
                                let names = mat_name_stored.get_value();
                                let display_name = match &ing.source {
                                    IngredientSource::Material  { material_id  } =>
                                        names.get(material_id).cloned().unwrap_or_else(|| material_id.clone()),
                                    IngredientSource::Blueprint { blueprint_id } =>
                                        names.get(blueprint_id).cloned().unwrap_or_else(|| blueprint_id.clone()),
                                };
                                let qty_per = ing.qty;
                                view! {
                                    <div class="flex items-center gap-2 text-xs ">
                                        <span class="w-40 truncate font-medium ">{display_name}</span>
                                        <span class="text-gray-400 ">
                                            "benötigt: "
                                            {move || {
                                                let c: f64 = cycles_input.get().parse().unwrap_or(1.0);
                                                format!("{:.0}", qty_per * c)
                                            }}
                                        </span>
                                        <input class="input w-20 text-right " type="number"
                                            min="0" step="1"
                                            value=move || inv.get()
                                            on:input=move |ev| inv.set(event_target_value(&ev))
                                            placeholder="0" />
                                        <span class="text-gray-400 ">"vorhanden"</span>
                                    </div>
                                }
                            }).collect_view()
                        }
                    </div>
                    <div class="flex gap-2 ">
                        <button class="btn-primary " on:click=move |_| {
                            let bp     = bp_for_hunt.get_value();
                            let inv    = inv_stored.get_value();
                            let cycles: f64 = cycles_input.get_untracked().parse().unwrap_or(1.0);
                            let names  = mat_name_stored.get_value();
                            let needs: Vec<HuntNeed> = bp.ingredients.iter().enumerate()
                                .map(|(i, ing)| {
                                    let already_have: f64 = inv[i].get_untracked().parse().unwrap_or(0.0);
                                    let item = match &ing.source {
                                        IngredientSource::Material  { material_id  } =>
                                            names.get(material_id).cloned().unwrap_or_else(|| material_id.clone()),
                                        IngredientSource::Blueprint { blueprint_id } =>
                                            names.get(blueprint_id).cloned().unwrap_or_else(|| blueprint_id.clone()),
                                    };
                                    HuntNeed { item, total_needed: ing.qty * cycles, already_have }
                                }).collect();
                            let plan = HuntPlan {
                                blueprint_id:   bp.id.clone(),
                                blueprint_name: bp.name.clone(),
                                craft_cycles:   cycles as u64,
                                needs,
                            };
                            if let Some(d) = db() {
                                let on_activated = on_activated.clone();
                                spawn_local(async move {
                                    let _ = d.set_active_hunt_plan(&plan).await;
                                    on_activated();
                                });
                            }
                            show_hunt.set(false);
                        }>"✅ Aktivieren"</button>
                        <button class="btn-secondary " on:click=move |_| show_hunt.set(false)>
                            "Abbrechen"
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

// ─── BlueprintForm ────────────────────────────────────────────────────────────

/// Ingredient-Formzeile: (material_id, qty)
#[derive(Clone, PartialEq)]
struct IngredientRow {
    material_id: String,
    qty: f64,
}

#[component]
fn BlueprintForm<F: Fn() + 'static + Clone>(
    initial: RwSignal<Option<Blueprint>>,
    materials: Vec<Material>,
    db: impl Fn() -> Option<Db> + 'static + Copy,
    on_done: F,
) -> impl IntoView {
    let bp_name  = RwSignal::new(String::new());
    let out_item = RwSignal::new(String::new());
    let out_qty  = RwSignal::new(String::from("1"));
    let out_tt   = RwSignal::new(String::new());
    let out_mu   = RwSignal::new(String::from("100"));
    let rows     = RwSignal::new(vec![IngredientRow { material_id: String::new(), qty: 1.0 }]);

    Effect::new(move |_| {
        if let Some(bp) = initial.get() {
            bp_name.set(bp.name.clone());
            out_item.set(bp.product_name.clone());
            out_qty.set(bp.output_qty.to_string());
            out_tt.set(bp.output_tt_value.to_string());
            out_mu.set(bp.markup_pct.to_string());
            rows.set(bp.ingredients.iter().map(|ing| IngredientRow {
                material_id: Blueprint::ingredient_id(ing).to_string(),
                qty: ing.qty,
            }).collect());
        }
    });

    let on_save = {
        let on_done = on_done.clone();
        move |_| {
            let id = initial.get_untracked().map(|b| b.id)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let ingredients: Vec<Ingredient> = rows.get_untracked().into_iter()
                .filter(|r| !r.material_id.is_empty())
                .map(|r| Ingredient {
                    source: IngredientSource::Material { material_id: r.material_id },
                    qty: r.qty,
                    sub_mode: SubMode::Buy,
                    markup_override_pct: None,
                })
                .collect();
            let bp = Blueprint {
                id,
                nexus_id: 0,
                name:            bp_name.get_untracked(),
                product_name:    out_item.get_untracked(),
                output_qty:      out_qty.get_untracked().parse().unwrap_or(1.0),
                output_tt_value: out_tt.get_untracked().parse().unwrap_or(0.0),
                markup_pct:      out_mu.get_untracked().parse().unwrap_or(100.0),
                ingredients,
                book: None,
                created_at_sec: crate::services::run_service::now_sec(),
            };
            if bp.name.is_empty() { return; }
            if let Some(d) = db() {
                let on_done = on_done.clone();
                spawn_local(async move { let _ = d.save_blueprint(&bp).await; on_done(); });
            }
        }
    };

    view! {
        <div class="border rounded p-4 bg-white space-y-3 text-sm ">
            <h3 class="font-medium ">
                {move || if initial.get().is_some() { "Blueprint bearbeiten" } else { "Neuer Blueprint" }}
            </h3>
            <div class="grid gap-2 sm:grid-cols-2 ">
                <label class="flex flex-col gap-1 ">"Blueprint-Name"
                    <input class="input " value=move || bp_name.get()
                        on:input=move |ev| bp_name.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"Output-Item"
                    <input class="input " value=move || out_item.get()
                        on:input=move |ev| out_item.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"Output-Menge"
                    <input class="input " type="number" step="0.01"
                        value=move || out_qty.get()
                        on:input=move |ev| out_qty.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"Output TT-Wert (PED)"
                    <input class="input " type="number" step="0.0001"
                        value=move || out_tt.get()
                        on:input=move |ev| out_tt.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"Output Markup %"
                    <input class="input " type="number" step="1"
                        value=move || out_mu.get()
                        on:input=move |ev| out_mu.set(event_target_value(&ev)) />
                </label>
            </div>

            <div>
                <div class="flex justify-between items-center mb-2 ">
                    <span class="font-medium ">"Zutaten"</span>
                    <span class="text-xs text-gray-400 ">"(Material muss im Materialien-Tab existieren)"</span>
                    <button class="btn-xs " on:click=move |_| rows.update(|v| v.push(
                        IngredientRow { material_id: String::new(), qty: 1.0 }
                    ))>"+ Zutat"</button>
                </div>
                <div class="text-xs text-gray-400 mb-1 grid grid-cols-4 gap-1 ">
                    <span class="col-span-3 ">"Material (ID oder Name)"</span>
                    <span>"Menge"</span>
                </div>

                // Material-Autocomplete-Liste für datalist
                <datalist id="material-list">
                    {materials.iter().map(|m| {
                        let id   = m.id.clone();
                        let name = m.name.clone();
                        view! { <option value=id>{name}</option> }
                    }).collect_view()}
                </datalist>

                {move || rows.get().into_iter().enumerate().map(|(i, row)| {
                    view! {
                        <div class="flex gap-1 mb-1 ">
                            <input class="input flex-1 " placeholder="material-id"
                                list="material-list"
                                value=row.material_id.clone()
                                on:input=move |ev| rows.update(|v| {
                                    if let Some(r) = v.get_mut(i) {
                                        r.material_id = event_target_value(&ev);
                                    }
                                }) />
                            <input class="input w-20 " type="number" step="0.01" min="0.01"
                                value=row.qty.to_string()
                                on:input=move |ev| rows.update(|v| {
                                    if let Some(r) = v.get_mut(i) {
                                        r.qty = event_target_value(&ev).parse().unwrap_or(1.0);
                                    }
                                }) />
                            <button class="btn-xs-danger "
                                on:click=move |_| rows.update(|v| { if i < v.len() { v.remove(i); } })>
                                "–"
                            </button>
                        </div>
                    }
                }).collect_view()}
            </div>

            <div class="space-x-2 ">
                <button class="btn-primary " on:click=on_save>"💾 Speichern"</button>
                <button class="btn-secondary " on:click=move |_| on_done.clone()()>"Abbrechen"</button>
            </div>
        </div>
    }
}

// ─── NexusImportDialog ────────────────────────────────────────────────────────

#[component]
fn NexusImportDialog<F: Fn() + 'static + Clone>(
    db:             impl Fn() -> Option<Db> + 'static + Copy,
    materials:      ReadSignal<Vec<Material>, leptos::prelude::LocalStorage>,
    blueprints:     ReadSignal<Vec<Blueprint>, leptos::prelude::LocalStorage>,
    import_book:    RwSignal<String>,
    import_status:  RwSignal<String>,
    importing:      RwSignal<bool>,
    on_done:        F,
) -> impl IntoView {

    let do_import = {
        let on_done = on_done.clone();
        move |_| {
            let Some(d) = db() else { return };
            let book = import_book.get_untracked();
            if book.is_empty() { return; }

            importing.set(true);
            import_status.set(format!("Lade \"{}\" von Nexus...", book));

            let existing_mats = materials.get_untracked();
            let existing_bps  = blueprints.get_untracked();
            let on_done       = on_done.clone();

            spawn_local(async move {
                match nexus_blueprints::fetch_by_book(&book).await {
                    Err(e) => {
                        import_status.set(format!("✗ {e}"));
                        importing.set(false);
                    }
                    Ok(nexus_bps) => {
                        let result  = nexus_blueprints::convert_blueprints(
                            nexus_bps, &existing_mats, &existing_bps,
                            crate::services::run_service::now_sec(),
                        );

                        // Materialien + Blueprints speichern
                        let mat_count = result.materials.len();
                        let bp_count  = result.blueprints.len();

                        if let Err(e) = d.save_materials_batch(&result.materials).await {
                            import_status.set(format!("✗ Material-Speicherfehler: {e:?}"));
                            importing.set(false);
                            return;
                        }
                        for bp in &result.blueprints {
                            if let Err(e) = d.save_blueprint(bp).await {
                                import_status.set(format!("✗ Blueprint-Speicherfehler: {e:?}"));
                                importing.set(false);
                                return;
                            }
                        }

                        import_status.set(format!(
                            "✅ {} Blueprints + {} Materialien importiert ({} übersprungen)",
                            bp_count, mat_count, result.skipped
                        ));
                        importing.set(false);
                        on_done();
                    }
                }
            });
        }
    };

    view! {
        <div class="border rounded p-4 bg-blue-50 space-y-3 text-sm ">
            <h3 class="font-semibold text-blue-800 ">"📥 Nexus Blueprint Import"</h3>
            <p class="text-xs text-blue-600 ">
                "Blueprints direkt von der Entropianexus-API laden. "
                "Materialien werden automatisch angelegt (Markup: 100%, anpassbar im Materialien-Tab)."
            </p>

            <div class="flex items-center gap-3 flex-wrap ">
                <label class="flex items-center gap-2 ">
                    "Blueprint-Book:"
                    <select class="input "
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            let val = ev.target().unwrap()
                                .dyn_into::<web_sys::HtmlSelectElement>().unwrap()
                                .value();
                            import_book.set(val);
                        }>
                        {KNOWN_BOOKS.iter().map(|(label, value)| {
                            let v = value.to_string();
                            let l = label.to_string();
                            let is_default = *value == KNOWN_BOOKS[0].1;
                            view! {
                                <option value=v selected=is_default>{l}</option>
                            }
                        }).collect_view()}
                    </select>
                </label>

                <button class="btn-primary "
                    disabled=move || importing.get()
                    on:click=do_import.clone()>
                    {move || if importing.get() { "Importiere..." } else { "Importieren" }}
                </button>

                <button class="btn-secondary "
                    on:click=move |_| on_done.clone()()>
                    "Abbrechen"
                </button>
            </div>

            {move || {
                let s = import_status.get();
                if s.is_empty() { return view! { <span></span> }.into_any(); }
                let color = if s.starts_with("✅") { "text-green-700" }
                            else if s.starts_with("✗") { "text-red-600" }
                            else { "text-blue-600" };
                view! { <p class=color>{s}</p> }.into_any()
            }}
        </div>
    }
}


// ─── CraftRunSection ──────────────────────────────────────────────────────────

#[component]
fn CraftRunSection() -> impl IntoView {
    use std::rc::Rc;
    use std::cell::RefCell;

    let (_db, set_db)          = signal_local::<Option<Db>>(None);
    let (bps, set_bps)         = signal_local::<Vec<Blueprint>>(vec![]);
    let (stock, set_stock)     = signal_local::<Vec<crate::domain::types::StockEntry>>(vec![]);
    let (all_mats, set_mats)   = signal_local::<Vec<Material>>(vec![]);
    let selected_book          = RwSignal::new(String::new());
    let selected_bp_id         = RwSignal::new(String::new());
    let success_rate_pct       = RwSignal::new(90.0f64);
    // (CalcResult, input_cost_per_attempt, markup_pct)
    let active_calc            = RwSignal::new(Option::<(CalcResult, f64, f64)>::None);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await {
                if let Ok(v) = d.get_all_blueprints().await {
                    let mut s = v; s.sort_by(|a,b| a.name.cmp(&b.name));
                    set_bps.set(s);
                }
                if let Ok(v) = d.get_all_stock().await { set_stock.set(v); }
                if let Ok(v) = d.get_all_materials().await { set_mats.set(v); }
                set_db.set(Some(d));
            }
        });
    });

    Effect::new(move |_| {
        let bp_id   = selected_bp_id.get();
        let sr      = success_rate_pct.get() / 100.0;
        let all_bps = bps.get();
        let mats    = all_mats.get();
        if bp_id.is_empty() { active_calc.set(None); return; }
        if let Some(bp) = all_bps.iter().find(|b| b.id == bp_id) {
            let params     = CalcParams { success_rate: sr, ..Default::default() };
            let result     = calc_one(bp, &all_bps, &mats, &params);
            let input_cost = result.cost_per_output * sr * bp.output_qty;
            let markup_pct = bp.markup_pct;
            active_calc.set(Some((result, input_cost, markup_pct)));
        } else {
            active_calc.set(None);
        }
    });

    // JsValue ist !Send → Rc<RefCell> für WASM single-thread
    let file_handle  = Rc::new(RefCell::new(Option::<wasm_bindgen::JsValue>::None));
    let has_file     = RwSignal::new(false);
    let offset       = RwSignal::new(0u64);
    let stats        = RwSignal::new(CraftRunStats::default());
    let attempts     = RwSignal::new(Vec::<crate::domain::types::CraftAttempt>::new());
    let running      = RwSignal::new(false);
    let status_msg   = RwSignal::new(String::new());
    let cancel_token = Rc::new(RefCell::new(Arc::new(AtomicBool::new(false))));

    let fh_pick  = file_handle.clone();
    let fh_start = file_handle.clone();
    let ct_start = cancel_token.clone();
    let ct_stop  = cancel_token.clone();

    let pick_file = move |_| {
        let fh = fh_pick.clone();
        spawn_local(async move {
            let handle = if fs_access::hasOpenFilePicker().unwrap_or(false) {
                match fs_access::pickFile().await {
                    Ok(h) => h,
                    Err(_) => { status_msg.set("Datei-Auswahl abgebrochen.".into()); return; }
                }
            } else {
                match fs_access::simpleInputPick().await {
                    Ok(h) => h,
                    Err(_) => { status_msg.set("Datei-Auswahl abgebrochen.".into()); return; }
                }
            };
            status_msg.set("Log-Datei gewählt. ▶ Starten drücken.".into());
            *fh.borrow_mut() = Some(handle);
            has_file.set(true);
        });
    };

    let start = move |_| {
        let handle = match fh_start.borrow().clone() {
            Some(h) => h,
            None => { status_msg.set("Zuerst Log-Datei wählen.".into()); return; }
        };
        stats.set(CraftRunStats::default());
        attempts.set(vec![]);
        offset.set(0);
        let tok = Arc::new(AtomicBool::new(false));
        *ct_start.borrow_mut() = tok.clone();
        running.set(true);
        status_msg.set("⏺ Craft-Run läuft...".into());
        start_craft_polling(handle, offset, stats, attempts, tok);
    };

    let stop = move |_| {
        ct_stop.borrow().store(true, Ordering::Relaxed);
        running.set(false);
        status_msg.set("⏹ Gestoppt.".into());
    };

    view! {
        <div class="border rounded p-4 bg-orange-50 border-orange-200 space-y-3 text-sm ">
            <h3 class="font-semibold text-orange-800 ">"🔨 Craft-Run – Live-Tracking"</h3>
            <p class="text-xs text-orange-700 ">
                "EU Chat-Log wählen → Starten → Crafting-Loot wird live ausgewertet. "
                "Erfolg = mind. ein Output-Item (kein Residue/Shrapnel). "
                "Residue zählt als Rückgewinnung (kein Totalverlust bei Fehlschlag)."
            </p>

            // ── Blueprint-Konfiguration ──────────────────────────────────────
            <div class="border rounded p-3 bg-white space-y-2 ">
                <div class="font-semibold text-xs text-gray-700 ">"Blueprint-Konfiguration"</div>
                <div class="flex flex-wrap gap-3 ">
                    <label class="flex flex-col gap-1 text-xs flex-1 min-w-36 ">
                        "Blueprint-Book"
                        <select class="input "
                            on:change=move |ev| {
                                selected_book.set(event_target_value(&ev));
                                selected_bp_id.set(String::new());
                            }>
                            <option value="">"Alle"</option>
                            {move || {
                                let mut books: Vec<String> = bps.get().into_iter()
                                    .filter_map(|bp| bp.book)
                                    .collect::<std::collections::HashSet<_>>()
                                    .into_iter()
                                    .collect();
                                books.sort();
                                books.into_iter().map(|book| {
                                    let b = book.clone();
                                    view!{ <option value=b>{book}</option> }
                                }).collect_view()
                            }}
                        </select>
                    </label>
                    <label class="flex flex-col gap-1 text-xs flex-1 min-w-48 ">
                        "Blueprint"
                        <select class="input "
                            on:change=move |ev| selected_bp_id.set(event_target_value(&ev))>
                            <option value="">"– keiner gewählt –"</option>
                            {move || {
                                let book = selected_book.get();
                                let sel  = selected_bp_id.get();
                                bps.get().into_iter()
                                    .filter(|bp| book.is_empty() || bp.book.as_deref() == Some(&book))
                                    .map(|bp| {
                                        let id  = bp.id.clone();
                                        let lbl = format!("{} → {}", bp.name, bp.product_name);
                                        let is_sel = sel == id;
                                        view!{ <option value=id selected=is_sel>{lbl}</option> }
                                    }).collect_view()
                            }}
                        </select>
                    </label>
                    <label class="flex flex-col gap-1 text-xs w-28 ">
                        "Erfolgsrate %"
                        <input type="number" class="input " min="50" max="100" step="1"
                            prop:value=move || success_rate_pct.get() as u32
                            on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    success_rate_pct.set(v.clamp(50.0, 100.0));
                                }
                            }
                        />
                    </label>
                </div>

                // Calc-Referenz
                {move || {
                    let Some((calc, input_cost, markup_pct)) = active_calc.get()
                    else { return view!{<span></span>}.into_any(); };
                    let sr          = success_rate_pct.get() / 100.0;
                    let expected_tt = calc.output_tt_value * sr;
                    let expected_mu = calc.output_tt_value * (markup_pct / 100.0) * sr;
                    let profit      = expected_mu - input_cost;
                    let profit_cls  = if profit >= 0.0 { "font-mono font-bold text-green-600" }
                                     else { "font-mono font-bold text-red-500" };
                    view!{
                        <div class="mt-2 border-t pt-2 text-xs ">
                            <div class="text-gray-500 mb-1 ">
                                "📊 " {calc.product_name.clone()} " – Calc-Referenz (theoretisch)"
                            </div>
                            <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 ">
                                <div class="border rounded p-2 bg-blue-50 ">
                                    <div class="text-gray-500 ">"Kosten/Versuch"</div>
                                    <div class="font-mono font-bold ">{format!("{:.4} PED", input_cost)}</div>
                                </div>
                                <div class="border rounded p-2 bg-blue-50 ">
                                    <div class="text-gray-500 ">"Erwart. TT-Return"</div>
                                    <div class="font-mono font-bold ">{format!("{:.4} PED", expected_tt)}</div>
                                </div>
                                <div class="border rounded p-2 bg-blue-50 ">
                                    <div class="text-gray-500 ">"Break-even MU"</div>
                                    <div class="font-mono font-bold ">{format!("{:.1}%", calc.breakeven)}</div>
                                </div>
                                <div class="border rounded p-2 bg-blue-50 ">
                                    <div class="text-gray-500 ">{format!("Profit/Craft bei {:.0}%", markup_pct)}</div>
                                    <div class=profit_cls>{format!("{:+.4} PED", profit)}</div>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}
            </div>

            // ── Materialcheckliste ────────────────────────────────────────────
            {move || {
                let bp_id = selected_bp_id.get();
                if bp_id.is_empty() { return view!{<span></span>}.into_any(); }
                let bp = match bps.get().into_iter().find(|b| b.id == bp_id) {
                    Some(b) => b, None => return view!{<span></span>}.into_any(),
                };
                let stock_map: std::collections::HashMap<String, f64> = stock.get()
                    .into_iter().map(|e| (e.id, e.qty)).collect();
                let mat_map: std::collections::HashMap<String, String> = all_mats.get()
                    .into_iter().map(|m| (m.id, m.name)).collect();
                let rows: Vec<(String, f64, f64)> = bp.ingredients.iter().map(|ing| {
                    let (id, name) = match &ing.source {
                        crate::domain::types::IngredientSource::Material { material_id } =>
                            (material_id.clone(), mat_map.get(material_id).cloned().unwrap_or_else(|| material_id.clone())),
                        crate::domain::types::IngredientSource::Blueprint { blueprint_id } =>
                            (blueprint_id.clone(), blueprint_id.clone()),
                    };
                    let in_stock = stock_map.get(&id).copied().unwrap_or(0.0);
                    (name, ing.qty, in_stock)
                }).collect();
                view!{
                    <div class="border rounded p-2 bg-white text-xs space-y-1 ">
                        <div class="font-semibold text-orange-800 ">
                            "📋 " {bp.product_name.clone()} " – Materialien (pro Craft)"
                        </div>
                        <table class="w-full border-collapse ">
                            <thead class="border-b ">
                                <tr>
                                    <th class="text-left p-1 ">"Material"</th>
                                    <th class="text-right p-1 ">"Benötigt"</th>
                                    <th class="text-right p-1 ">"Im Lager"</th>
                                    <th class="text-right p-1 ">"Fehlend"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows.into_iter().map(|(name, needed, in_stock)| {
                                    let deficit = (needed - in_stock).max(0.0);
                                    let row_cls = if deficit > 0.0 { "border-b bg-red-50" } else { "border-b bg-green-50" };
                                    view!{
                                        <tr class=row_cls>
                                            <td class="p-1 ">{name}</td>
                                            <td class="p-1 text-right font-mono ">{format!("{:.0}", needed)}</td>
                                            <td class="p-1 text-right font-mono ">{format!("{:.0}", in_stock)}</td>
                                            <td class="p-1 text-right font-mono ">
                                                {if deficit > 0.0 {
                                                    view!{<span class="text-red-600 font-bold ">{format!("−{:.0}", deficit)}</span>}.into_any()
                                                } else {
                                                    view!{<span class="text-green-600 ">"✓"</span>}.into_any()
                                                }}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}

            // ── Steuerung ────────────────────────────────────────────────────
            <div class="flex gap-2 flex-wrap items-center ">
                <button class="btn-secondary " on:click=pick_file
                    disabled=move || running.get()>
                    "📂 Log-Datei wählen"
                </button>
                <button class="btn-primary " on:click=start
                    disabled=move || running.get() || !has_file.get()>
                    "▶ Starten"
                </button>
                <button class="btn-secondary " on:click=stop
                    disabled=move || !running.get()>
                    "⏹ Stopp"
                </button>
                <span class="text-xs text-gray-500 ">{move || status_msg.get()}</span>
            </div>

            // ── Live-Stats ───────────────────────────────────────────────────
            {move || {
                let s = stats.get();
                if s.attempts == 0 { return view!{<span></span>}.into_any(); }

                let n_attempts  = s.attempts;
                let n_successes = s.successes;
                let n_failures  = s.failures;
                let sr          = s.success_rate();
                let total_out   = s.total_output_ped;
                let total_res   = s.total_residue_ped;
                let total_shr   = s.total_shrapnel_ped;
                let total_ret   = total_out + total_res + total_shr;
                let n           = n_attempts as f64;
                let avg_ret     = total_ret / n;

                let mut sorted_items: Vec<(String, u64, f64)> = s.output_items.iter()
                    .map(|(name, (qty, ped))| (name.clone(), *qty, *ped))
                    .collect();
                sorted_items.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

                let sr_cls = if sr >= 90.0 { "font-bold text-green-600" }
                             else if sr >= 80.0 { "font-bold text-yellow-600" }
                             else { "font-bold text-red-500" };

                let pnl_section = active_calc.get().map(|(calc, input_cost, markup_pct)| {
                    let diff       = avg_ret - input_cost;
                    let total_cost = n * input_cost;
                    let pl_tt      = total_ret - total_cost;
                    let eff_ret    = total_out * (markup_pct / 100.0) + total_res + total_shr;
                    let pl_mu      = eff_ret - total_cost;
                    let diff_cls   = if diff  >= 0.0 { "font-mono font-bold text-green-600" } else { "font-mono font-bold text-red-500" };
                    let pl_tt_cls  = if pl_tt >= 0.0 { "font-mono font-bold text-green-600" } else { "font-mono font-bold text-red-500" };
                    let pl_mu_cls  = if pl_mu >= 0.0 { "font-mono font-bold text-green-600" } else { "font-mono font-bold text-red-500" };
                    view! {
                        <div class="space-y-2 border rounded p-3 bg-white text-xs ">
                            <div class="font-semibold text-gray-700 ">"Ø pro Versuch"</div>
                            <div class="grid grid-cols-3 gap-2 ">
                                <div>
                                    <div class="text-gray-500 ">"Tats. Return (inkl. Residue)"</div>
                                    <div class="font-mono font-bold ">{format!("{:.4} PED", avg_ret)}</div>
                                </div>
                                <div>
                                    <div class="text-gray-500 ">"Theoret. Kosten"</div>
                                    <div class="font-mono font-bold ">{format!("{:.4} PED", input_cost)}</div>
                                </div>
                                <div>
                                    <div class="text-gray-500 ">"Differenz"</div>
                                    <div class=diff_cls>{format!("{:+.4} PED", diff)}</div>
                                </div>
                            </div>
                            <div class="border-t pt-2 font-semibold text-gray-700 ">"Run gesamt"</div>
                            <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 ">
                                <div>
                                    <div class="text-gray-500 ">"Kosten gesamt"</div>
                                    <div class="font-mono font-bold ">{format!("{:.2} PED", total_cost)}</div>
                                </div>
                                <div>
                                    <div class="text-gray-500 ">"TT-Return gesamt"</div>
                                    <div class="font-mono font-bold ">{format!("{:.2} PED", total_ret)}</div>
                                </div>
                                <div>
                                    <div class="text-gray-500 ">"P&L (TT-Basis)"</div>
                                    <div class=pl_tt_cls>{format!("{:+.2} PED", pl_tt)}</div>
                                </div>
                                <div>
                                    <div class="text-gray-500 ">{format!("P&L bei {:.0}% MU", markup_pct)}</div>
                                    <div class=pl_mu_cls>{format!("{:+.2} PED", pl_mu)}</div>
                                </div>
                            </div>
                        </div>
                    }
                });

                view! {
                    <div class="space-y-3 ">
                        <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 ">
                            <div class="border rounded p-2 bg-white text-center ">
                                <div class="text-lg font-bold ">{n_attempts}</div>
                                <div class="text-xs text-gray-500 ">"Versuche"</div>
                            </div>
                            <div class="border rounded p-2 bg-white text-center ">
                                <div class="text-lg font-bold text-green-600 ">{n_successes}</div>
                                <div class="text-xs text-gray-500 ">"Erfolge"</div>
                            </div>
                            <div class="border rounded p-2 bg-white text-center ">
                                <div class="text-lg font-bold text-red-500 ">{n_failures}</div>
                                <div class="text-xs text-gray-500 ">"Fehlschläge"</div>
                            </div>
                            <div class="border rounded p-2 bg-white text-center ">
                                <div class=sr_cls>{format!("{:.1}%", sr)}</div>
                                <div class="text-xs text-gray-500 ">"Erfolgsrate"</div>
                            </div>
                        </div>

                        {pnl_section}

                        <div class="grid grid-cols-3 gap-2 text-xs ">
                            <div class="border rounded p-2 bg-white ">
                                <div class="font-semibold ">"Output (TT)"</div>
                                <div class="font-mono ">{format!("{:.4} PED", total_out)}</div>
                            </div>
                            <div class="border rounded p-2 bg-white ">
                                <div class="font-semibold ">"Residue zurück"</div>
                                <div class="font-mono text-blue-600 ">{format!("{:.4} PED", total_res)}</div>
                            </div>
                            <div class="border rounded p-2 bg-white ">
                                <div class="font-semibold ">"Shrapnel"</div>
                                <div class="font-mono ">{format!("{:.4} PED", total_shr)}</div>
                            </div>
                        </div>

                        {if sorted_items.is_empty() { None } else {
                            Some(view! {
                                <div>
                                    <div class="text-xs font-semibold text-gray-600 mb-1 ">"Output-Items:"</div>
                                    <div class="flex flex-wrap gap-2 ">
                                        {sorted_items.into_iter().map(|(name, qty, ped)| view! {
                                            <span class="border rounded px-2 py-0.5 bg-white text-xs ">
                                                {name} " ×" {qty}
                                                <span class="ml-1 text-gray-400 ">
                                                    {format!("({:.4} PED)", ped)}
                                                </span>
                                            </span>
                                        }).collect_view()}
                                    </div>
                                </div>
                            })
                        }}

                        <details>
                            <summary class="cursor-pointer text-xs text-gray-500 ">
                                "Letzte Versuche (" {move || attempts.get().len()} ")"
                            </summary>
                            <div class="mt-1 space-y-1 ">
                            {move || {
                                let mut att = attempts.get();
                                att.reverse();
                                att.into_iter().take(8).map(|a| {
                                    let icon = if a.success { "✅" } else { "❌" };
                                    let desc = if a.success {
                                        a.output_items.iter()
                                            .map(|(n, q, _)| format!("{n} ×{q}"))
                                            .collect::<Vec<_>>().join(", ")
                                    } else {
                                        format!("Residue {:.4} PED", a.residue_ped + a.shrapnel_ped)
                                    };
                                    view! {
                                        <div class="text-xs flex gap-2 ">
                                            <span>{icon}</span>
                                            <span class="text-gray-600 ">{desc}</span>
                                            <span class="ml-auto font-mono text-gray-400 ">
                                                {format!("{:.4} PED", a.total_value_ped)}
                                            </span>
                                        </div>
                                    }
                                }).collect_view()
                            }}
                            </div>
                        </details>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
