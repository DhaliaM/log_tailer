use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::*;

use crate::domain::types::SavedRun;
use crate::persistence::idb::Db;
use std::collections::HashMap;

#[component]
pub fn HistoryPanel() -> impl IntoView {
    let (db, set_db) = signal_local::<Option<Db>>(None);
    let (runs, set_runs) = signal_local::<Vec<SavedRun>>(vec![]);
    let (mu_map, set_mu_map) = signal_local::<HashMap<String, f64>>(HashMap::new());
    let filter_creature = RwSignal::new(String::new());
    let filter_from = RwSignal::new(String::new()); // "YYYY-MM-DD"
    let filter_to   = RwSignal::new(String::new()); // "YYYY-MM-DD"

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await {
                set_db.set(Some(d));
            }
        });
    });

    let reload = {
        let db = db.clone();
        move || {
            if let Some(d) = db.get() {
                let d2 = d.clone();
                spawn_local(async move {
                    if let Ok(mut rs) = d.get_all_runs().await {
                        rs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
                        set_runs.set(rs);
                    }
                });
                spawn_local(async move {
                    if let Ok(items) = d2.get_all_loot_item_configs().await {
                        let map: HashMap<String, f64> = items.into_iter().map(|c| (c.name, c.mu_pct)).collect();
                        set_mu_map.set(map);
                    }
                });
            }
        }
    };

    Effect::new({
        let r = reload.clone();
        move |_| { let _ = db.get(); r(); }
    });

    view! {
        <div class="space-y-4 max-w-4xl ">
            <h2 class="text-lg font-semibold ">"📊 History"</h2>

            <div class="flex gap-2 items-center flex-wrap text-sm ">
                <label>"Kreatur:"
                    <select class="input ml-1 "
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            filter_creature.set(
                                ev.target().unwrap()
                                  .dyn_into::<web_sys::HtmlSelectElement>().unwrap()
                                  .value()
                            );
                        }>
                        <option value="">"— Alle —"</option>
                        {move || {
                            let mut creatures: Vec<String> = runs.get()
                                .into_iter()
                                .map(|r| r.config.creature)
                                .filter(|c| !c.is_empty())
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();
                            creatures.sort();
                            creatures.into_iter().map(|c| {
                                let c2 = c.clone();
                                view! { <option value=c2>{c}</option> }
                            }).collect_view()
                        }}
                    </select>
                </label>
                <label>"Von:"
                    <input type="date" class="input ml-1 "
                        on:input=move |ev| filter_from.set(event_target_value(&ev))
                    />
                </label>
                <label>"Bis:"
                    <input type="date" class="input ml-1 "
                        on:input=move |ev| filter_to.set(event_target_value(&ev))
                    />
                </label>
                {move || {
                    let fc = filter_creature.get();
                    let ff = filter_from.get();
                    let ft = filter_to.get();
                    if fc.is_empty() && ff.is_empty() && ft.is_empty() {
                        return view!{<span></span>}.into_any();
                    }
                    view!{
                        <button class="btn-xs " on:click=move |_| {
                            filter_creature.set(String::new());
                            filter_from.set(String::new());
                            filter_to.set(String::new());
                        }>"✕ Filter zurücksetzen"</button>
                    }.into_any()
                }}
            </div>

            {move || {
                let fc = filter_creature.get();
                let mu = mu_map.get();
                let from_ts: u64 = parse_date_to_ts(&filter_from.get());
                let to_ts:   u64 = parse_date_to_ts_end(&filter_to.get());
                let rs: Vec<_> = runs.get().into_iter()
                    .filter(|r| fc.is_empty() || r.config.creature.to_lowercase().contains(&fc.to_lowercase()))
                    .filter(|r| r.started_at >= from_ts)
                    .filter(|r| to_ts == 0 || r.started_at <= to_ts)
                    .collect();
                if rs.is_empty() {
                    return view!{<div class="text-gray-400 italic ">"Keine Runs."</div>}.into_any();
                }

                let total_kills: u64 = rs.iter().map(|r| r.stats.kills).sum();
                let total_loot: f64  = rs.iter().map(|r| r.stats.total_loot_value_ped).sum();
                let total_cost: f64  = rs.iter().map(|r| r.total_cost_ped).sum();
                let total_loot_mu: f64 = rs.iter().map(|r| loot_with_mu(r, &mu)).sum();
                let total_profit_tt: f64 = total_loot - total_cost;
                let total_profit_mu: f64 = total_loot_mu - total_cost;
                let return_tt: f64 = if total_cost > 0.0 { total_loot / total_cost * 100.0 } else { 0.0 };
                let return_mu: f64 = if total_cost > 0.0 { total_loot_mu / total_cost * 100.0 } else { 0.0 };

                let mu_clone = mu.clone();
                view! {
                    <div>
                        // Aggregat-Übersicht
                        <div class="bg-blue-50 p-3 rounded border border-blue-200 text-sm grid grid-cols-2 sm:grid-cols-4 gap-3 mb-4 ">
                            <div class="text-center ">
                                <div class="font-bold text-lg ">{total_kills}</div>
                                <div class="text-xs text-gray-500 ">"Kills gesamt"</div>
                            </div>
                            <div class="text-center ">
                                <div class="font-bold ">{format!("{:.2} PED", total_cost)}</div>
                                {if total_kills > 0 {
                                    let cpk = total_cost / total_kills as f64;
                                    Some(view!{ <div class="text-xs text-gray-600 ">{format!("{:.4} PED/Kill", cpk)}</div> })
                                } else { None }}
                                <div class="text-xs text-gray-500 ">"Kosten gesamt"</div>
                            </div>
                            <div class="text-center ">
                                <div class="text-gray-600 ">{format!("{:.2}", total_loot)} " TT"</div>
                                <div class="font-semibold text-blue-700 ">{format!("{:.2} PED", total_loot_mu)} " +MU"</div>
                                <div class="text-xs text-gray-400 ">"Loot gesamt"</div>
                            </div>
                            <div class="text-center ">
                                <div class={if return_tt >= 100.0 {"text-green-600"} else {"text-red-500"}}>
                                    {format!("{:.1}%", return_tt)} " TT"
                                </div>
                                <div class={if return_mu >= 100.0 {"font-bold text-green-700"} else {"font-bold text-red-600"}}>
                                    {format!("{:.1}%", return_mu)} " +MU"
                                </div>
                                <div class={if total_profit_mu >= 0.0 {"text-xs text-green-600"} else {"text-xs text-red-500"}}>
                                    {format!("{:+.2} PED", total_profit_mu)}
                                </div>
                                <div class="text-xs text-gray-400 ">"Return / Profit"</div>
                            </div>
                        </div>

                        // Aggregat-Zeile TT-Verlust/Gewinn
                        <div class="flex gap-4 text-sm mb-3 ">
                            <span class="text-gray-600 ">"Profit TT: "
                                <span class={if total_profit_tt >= 0.0 {"text-green-600 font-semibold"} else {"text-red-500 font-semibold"}}>
                                    {format!("{:+.2} PED", total_profit_tt)}
                                </span>
                            </span>
                            <span class="text-gray-600 ">"Profit+MU: "
                                <span class={if total_profit_mu >= 0.0 {"text-green-700 font-bold"} else {"text-red-600 font-bold"}}>
                                    {format!("{:+.2} PED", total_profit_mu)}
                                </span>
                            </span>
                        </div>

                        <ReturnVerlauf runs=rs.clone() mu_map=mu_clone.clone() />

                        <RoiRanking runs=rs.clone() mu_map=mu_clone.clone() />

                        <div class="space-y-2 mt-4 ">
                            {rs.into_iter().map(|r| {
                                let mu2 = mu_clone.clone();
                                let db2 = db.clone();
                                let reload2 = reload.clone();
                                view!{ <RunRow run=r mu_map=mu2 db=db2 on_delete=reload2 /> }
                            }).collect_view()}
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ─── Hilfsfunktion: Loot+MU für einen Run ─────────────────────────────────────

fn loot_with_mu(run: &SavedRun, mu_map: &HashMap<String, f64>) -> f64 {
    if run.stats.loot_items.is_empty() {
        return run.stats.total_loot_value_ped;
    }
    run.stats.loot_items.iter()
        .map(|(name, agg)| {
            let mu = mu_map.get(name).copied().unwrap_or(100.0);
            agg.total_value_ped * mu / 100.0
        })
        .sum()
}

// ─── RunRow ───────────────────────────────────────────────────────────────────

#[component]
fn RunRow<F: Fn() + 'static + Clone>(
    run: SavedRun,
    mu_map: HashMap<String, f64>,
    db: ReadSignal<Option<Db>, leptos::prelude::LocalStorage>,
    on_delete: F,
) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let date = format_ts(run.started_at);
    let duration = run.stopped_at.saturating_sub(run.started_at);
    let run_id = run.id;
    let note = run.note.clone();

    let loot_mu   = loot_with_mu(&run, &mu_map);
    let profit_mu = loot_mu - run.total_cost_ped;
    let return_mu = if run.total_cost_ped > 0.0 { loot_mu / run.total_cost_ped * 100.0 } else { 0.0 };

    let ret_tt_cls  = if run.return_pct_tt >= 100.0 { "text-green-600" }   else { "text-red-500" };
    let ret_mu_cls  = if return_mu       >= 100.0   { "text-green-700 font-semibold" } else { "text-red-600 font-semibold" };
    let profit_cls  = if run.profit_ped  >= 0.0     { "text-green-600" }   else { "text-red-500" };
    let profmu_cls  = if profit_mu       >= 0.0     { "text-green-700 font-semibold" } else { "text-red-600 font-semibold" };

    view! {
        <div class="border rounded text-sm ">
            <div
                class="flex flex-wrap gap-x-4 gap-y-1 p-3 cursor-pointer hover:bg-gray-50 "
                on:click=move |_| expanded.update(|v| *v = !*v)
            >
                <span class="font-medium ">{date}</span>
                <span class="text-gray-600 ">{run.config.creature.clone()}</span>
                <span class="text-gray-500 ">{run.stats.kills} " Kills"</span>
                <span>"Kosten: " {format!("{:.2} PED", run.total_cost_ped)}</span>
                {if run.stats.kills > 0 {
                    let cpk = run.total_cost_ped / run.stats.kills as f64;
                    Some(view!{ <span class="text-gray-500 ">{format!("{:.4} PED/Kill", cpk)}</span> })
                } else { None }}
                <span>"TT: " {format!("{:.2}", run.stats.total_loot_value_ped)}</span>
                <span>"MU: " {format!("{:.2} PED", loot_mu)}</span>
                <span class=ret_tt_cls>{format!("{:.1}% TT", run.return_pct_tt)}</span>
                <span class=ret_mu_cls>{format!("{:.1}% +MU", return_mu)}</span>
                <span class=profit_cls>{format!("{:+.2}TT", run.profit_ped)}</span>
                <span class=profmu_cls>{format!("{:+.2} PED+MU", profit_mu)}</span>
                <span class="ml-auto text-gray-400 ">{if expanded.get() {"▲"} else {"▼"}}</span>
            </div>

            <div style=move || if expanded.get() { "" } else { "display:none" }>
                <div class="border-t p-3 space-y-2 text-xs bg-gray-50 ">
                    // Notiz
                    {note.as_ref().map(|n| view!{
                        <div class="bg-yellow-50 border border-yellow-200 rounded px-2 py-1 text-xs italic text-gray-700 ">
                            "📝 " {n.clone()}
                        </div>
                    })}
                    // Löschen-Button
                    <div class="flex justify-end ">
                        <button
                            class="btn-danger text-xs px-3 py-1 rounded "
                            on:click=move |_| {
                                if let Some(d) = db.get() {
                                    let on_del = on_delete.clone();
                                    spawn_local(async move {
                                        let _ = d.delete_run(run_id).await;
                                        on_del();
                                    });
                                }
                            }
                        >"🗑 Run löschen"</button>
                    </div>
                    <div class="grid grid-cols-2 sm:grid-cols-3 gap-2 ">
                        <div>"Dauer: " {format!("{} min {} s", duration / 60, duration % 60)}</div>
                        <div>"Schüsse: " {run.stats.total_shots}</div>
                        <div>"Crits: " {run.stats.player_crit_hits}</div>
                        <div>"Ammo: " {format!("{:.4} PED", run.ammo_cost_ped)}</div>
                        <div>"Armor Decay: " {format!("{:.4} PED", run.armor_decay_ped)}</div>
                        <div>"FAP Decay: " {format!("{:.4} PED", run.fap_decay_ped)}</div>
                        {run.config.enhancer_cost_ped.filter(|&v| v > 0.0).map(|v| view!{
                            <div>"Enhancer: " {format!("{:.4} PED", v)}</div>
                        })}
                        <div class="font-semibold col-span-2 sm:col-span-3 border-t pt-1 mt-1 ">
                            "Kosten gesamt: " {format!("{:.4} PED", run.total_cost_ped)}
                            {if run.stats.kills > 0 {
                                let cpk = run.total_cost_ped / run.stats.kills as f64;
                                Some(view!{ <span class="ml-3 font-normal text-gray-500 ">"(" {format!("{:.4} PED/Kill", cpk)} ")"</span> })
                            } else { None }}
                        </div>
                    </div>

                    {if run.stats.skill_gains.is_empty() { None } else {
                        let mut skills: Vec<_> = run.stats.skill_gains.iter().collect();
                        skills.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
                        Some(view! {
                            <div>
                                <div class="font-semibold mb-1 ">"Skill-Gains:"</div>
                                <div class="flex flex-wrap gap-2 ">
                                    {skills.into_iter().map(|(k, v)| view!{
                                        <span class="bg-blue-100 px-2 py-0.5 rounded ">
                                            {format!("{}: +{:.4}", k, v)}
                                        </span>
                                    }).collect_view()}
                                </div>
                            </div>
                        })
                    }}

                    {if run.stats.loot_items.is_empty() { None } else {
                        let mut items: Vec<_> = run.stats.loot_items.iter().collect();
                        items.sort_by(|a, b| b.1.total_value_ped.partial_cmp(&a.1.total_value_ped)
                            .unwrap_or(std::cmp::Ordering::Equal));
                        let mu_map2 = mu_map.clone();
                        Some(view! {
                            <table class="w-full border-collapse ">
                                <thead class="border-b ">
                                    <tr>
                                        <th class="text-left pr-2 ">"Item"</th>
                                        <th class="text-right pr-2 ">"TT"</th>
                                        <th class="text-right pr-2 ">"MU%"</th>
                                        <th class="text-right pr-2 ">"Wert+MU"</th>
                                        <th class="text-right ">"Anz."</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|(name, agg)| {
                                        let mu = mu_map2.get(name).copied().unwrap_or(100.0);
                                        let val_mu = agg.total_value_ped * mu / 100.0;
                                        view!{
                                            <tr class="border-b last:border-0 ">
                                                <td class="pr-2 ">{name.clone()}</td>
                                                <td class="text-right pr-2 ">{format!("{:.4}", agg.total_value_ped)}</td>
                                                <td class="text-right pr-2 text-gray-400 ">{format!("{:.0}%", mu)}</td>
                                                <td class="text-right pr-2 font-semibold ">{format!("{:.4}", val_mu)}</td>
                                                <td class="text-right ">{agg.event_count}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        })
                    }}
                </div>
            </div>
        </div>
    }
}

// ─── Return-Verlauf ───────────────────────────────────────────────────────────

#[component]
fn ReturnVerlauf(runs: Vec<SavedRun>, mu_map: HashMap<String, f64>) -> impl IntoView {
    let show_yearly = RwSignal::new(false);

    view! {
        <div class="mb-4 ">
            <div class="flex items-center gap-3 mb-2 ">
                <h3 class="font-semibold text-sm ">"Return-Verlauf"</h3>
                <div class="flex gap-1 text-xs ">
                    <button
                        class=move || if !show_yearly.get() { "px-2 py-0.5 rounded bg-blue-600 text-white" } else { "px-2 py-0.5 rounded bg-gray-100 text-gray-600" }
                        on:click=move |_| show_yearly.set(false)
                    >"Monatlich"</button>
                    <button
                        class=move || if show_yearly.get() { "px-2 py-0.5 rounded bg-blue-600 text-white" } else { "px-2 py-0.5 rounded bg-gray-100 text-gray-600" }
                        on:click=move |_| show_yearly.set(true)
                    >"Jährlich"</button>
                </div>
            </div>

            {move || {
                // Gruppierung
                #[derive(Default)]
                struct Period {
                    run_count: u64,
                    cost: f64,
                    loot_tt: f64,
                    loot_mu: f64,
                    kills: u64,
                }

                let mut map: std::collections::BTreeMap<String, Period> = std::collections::BTreeMap::new();
                for r in &runs {
                    let key = if show_yearly.get() {
                        ts_to_year_str(r.started_at)
                    } else {
                        ts_to_month_str(r.started_at)
                    };
                    let e = map.entry(key).or_default();
                    e.run_count += 1;
                    e.cost      += r.total_cost_ped;
                    e.loot_tt   += r.stats.total_loot_value_ped;
                    e.loot_mu   += loot_with_mu(r, &mu_map);
                    e.kills     += r.stats.kills;
                }

                if map.is_empty() {
                    return view!{<div></div>}.into_any();
                }

                // Sortiert absteigend (neueste zuerst)
                let periods: Vec<(String, Period)> = map.into_iter().rev().collect();
                // max return für Balken-Skalierung
                let max_ret = periods.iter().map(|(_, p)| {
                    if p.cost > 0.0 { p.loot_mu / p.cost * 100.0 } else { 0.0 }
                }).fold(0.0f64, f64::max).max(110.0);

                view! {
                    <div class="overflow-x-auto ">
                        <table class="w-full text-xs border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-1.5 ">"Zeitraum"</th>
                                    <th class="text-right p-1.5 ">"Runs"</th>
                                    <th class="text-right p-1.5 ">"Kills"</th>
                                    <th class="text-right p-1.5 ">"Kosten"</th>
                                    <th class="text-right p-1.5 ">"Loot TT"</th>
                                    <th class="text-right p-1.5 ">"Loot +MU"</th>
                                    <th class="text-right p-1.5 ">"Return TT"</th>
                                    <th class="text-right p-1.5 ">"Return +MU"</th>
                                    <th class="text-left p-1.5 min-w-24 ">"Verlauf"</th>
                                    <th class="text-right p-1.5 ">"Profit"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {periods.into_iter().map(|(period, p)| {
                                    let ret_tt = if p.cost > 0.0 { p.loot_tt / p.cost * 100.0 } else { 0.0 };
                                    let ret_mu = if p.cost > 0.0 { p.loot_mu / p.cost * 100.0 } else { 0.0 };
                                    let profit = p.loot_mu - p.cost;

                                    let bar_pct = (ret_mu / max_ret * 100.0).min(100.0);
                                    let bar_cls = if ret_mu >= 100.0 { "bg-green-500" } else { "bg-red-400" };
                                    // 100%-Linie relativ zur Bar-Breite
                                    let marker_pct = (100.0 / max_ret * 100.0).min(100.0);

                                    let ret_tt_cls  = if ret_tt >= 100.0 { "text-green-600" } else { "text-red-500" };
                                    let ret_mu_cls  = if ret_mu >= 100.0 { "text-green-700 font-semibold" } else { "text-red-600 font-semibold" };
                                    let profit_cls  = if profit >= 0.0 { "text-green-600" } else { "text-red-500" };

                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-1.5 font-medium ">{period}</td>
                                            <td class="text-right p-1.5 text-gray-500 ">{p.run_count}</td>
                                            <td class="text-right p-1.5 text-gray-500 ">{p.kills}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.2}", p.cost)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.2}", p.loot_tt)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.2}", p.loot_mu)}</td>
                                            <td class=format!("text-right p-1.5 {ret_tt_cls}")>{format!("{:.1}%", ret_tt)}</td>
                                            <td class=format!("text-right p-1.5 {ret_mu_cls}")>{format!("{:.1}%", ret_mu)}</td>
                                            <td class="p-1.5 ">
                                                // Balken-Chart mit 100%-Markierung
                                                <div class="relative h-3 bg-gray-100 rounded w-full " style="min-width:80px">
                                                    <div
                                                        class=format!("{bar_cls} h-3 rounded")
                                                        style=format!("width:{:.1}%", bar_pct)
                                                    ></div>
                                                    // 100%-Linie
                                                    <div
                                                        class="absolute top-0 bottom-0 w-px bg-gray-400 "
                                                        style=format!("left:{:.1}%", marker_pct)
                                                    ></div>
                                                </div>
                                            </td>
                                            <td class=format!("text-right p-1.5 {profit_cls}")>{format!("{:+.2}", profit)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                        <div class="text-xs text-gray-400 mt-1 ">"Strich = 100% Return | Balken = Return+MU | Profit in PED"</div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn ts_to_month_str(ts: u64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    let doy  = days % 365;
    let month_days: [u64; 12] = [31,28,31,30,31,30,31,31,30,31,30,31];
    let mut remaining = doy;
    let mut month = 1u64;
    for days_in_m in month_days.iter() {
        if remaining < *days_in_m { break; }
        remaining -= days_in_m;
        month += 1;
    }
    format!("{:04}-{:02}", year, month.min(12))
}

fn ts_to_year_str(ts: u64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    format!("{:04}", year)
}

// ─── ROI-Ranking ─────────────────────────────────────────────────────────────

#[component]
fn RoiRanking(runs: Vec<SavedRun>, mu_map: HashMap<String, f64>) -> impl IntoView {
    #[derive(Default)]
    struct CreatureStat {
        run_count: u64,
        total_cost: f64,
        total_loot_tt: f64,
        total_loot_mu: f64,
        total_kills: u64,
        total_seconds: u64,
    }

    let mut map: HashMap<String, CreatureStat> = HashMap::new();
    for r in &runs {
        let e = map.entry(r.config.creature.clone()).or_default();
        e.run_count     += 1;
        e.total_cost    += r.total_cost_ped;
        e.total_loot_tt += r.stats.total_loot_value_ped;
        e.total_loot_mu += loot_with_mu(r, &mu_map);
        e.total_kills   += r.stats.kills;
        e.total_seconds += r.stopped_at.saturating_sub(r.started_at);
    }

    let mut ranking: Vec<(String, CreatureStat)> = map.into_iter().collect();
    ranking.sort_by(|a, b| {
        let ra = if a.1.total_cost > 0.0 { a.1.total_loot_mu / a.1.total_cost } else { 0.0 };
        let rb = if b.1.total_cost > 0.0 { b.1.total_loot_mu / b.1.total_cost } else { 0.0 };
        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
    });

    view! {
        <div>
            <h3 class="font-semibold text-sm mb-2 ">"Kreatur-Ranking (nach Return+MU)"</h3>
            <div class="space-y-1 ">
                {ranking.into_iter().enumerate().map(|(i, (creature, stat))| {
                    let ret_tt = if stat.total_cost > 0.0 { stat.total_loot_tt / stat.total_cost * 100.0 } else { 0.0 };
                    let ret_mu = if stat.total_cost > 0.0 { stat.total_loot_mu / stat.total_cost * 100.0 } else { 0.0 };
                    let profit_mu = stat.total_loot_mu - stat.total_cost;
                    let kills_h = if stat.total_seconds > 0 {
                        stat.total_kills as f64 / (stat.total_seconds as f64 / 3600.0)
                    } else { 0.0 };
                    view! {
                        <div class="text-xs flex gap-3 p-2 bg-white border rounded flex-wrap ">
                            <span class="font-bold text-gray-400 w-4 shrink-0 ">{i + 1}"."</span>
                            <span class="font-medium min-w-24 ">{creature}</span>
                            <span class="text-gray-500 ">{stat.run_count} " Runs · " {stat.total_kills} " Kills"</span>
                            <span class={if ret_tt >= 100.0 {"text-green-600"} else {"text-red-500"}}>
                                {format!("TT {:.1}%", ret_tt)}
                            </span>
                            <span class={if ret_mu >= 100.0 {"text-green-700 font-semibold"} else {"text-red-600 font-semibold"}}>
                                {format!("+MU {:.1}%", ret_mu)}
                            </span>
                            <span class={if profit_mu >= 0.0 {"text-green-600"} else {"text-red-500"}}>
                                {format!("{:+.2} PED", profit_mu)}
                            </span>
                            <span class="text-gray-400 ">{format!("{:.0} Kills/h", kills_h)}</span>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

fn parse_date_to_ts(s: &str) -> u64 {
    if s.len() < 10 { return 0; }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 { return 0; }
    let y: u64 = parts[0].parse().unwrap_or(0);
    let m: u64 = parts[1].parse().unwrap_or(0);
    let d: u64 = parts[2].parse().unwrap_or(0);
    if y == 0 || m == 0 || d == 0 { return 0; }
    let years_since_epoch = y.saturating_sub(1970);
    let leap_days = years_since_epoch / 4;
    let month_days: [u64; 12] = [31,28,31,30,31,30,31,31,30,31,30,31];
    let day_of_year: u64 = month_days[..((m-1) as usize).min(11)].iter().sum::<u64>() + d - 1;
    let days = years_since_epoch * 365 + leap_days + day_of_year;
    days * 86400
}

fn parse_date_to_ts_end(s: &str) -> u64 {
    let ts = parse_date_to_ts(s);
    if ts == 0 { 0 } else { ts + 86399 }
}

fn format_ts(ts: u64) -> String {
    let secs  = ts % 60;
    let mins  = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    let days  = ts / 86400;
    let year  = 1970 + days / 365;
    let doy   = days % 365;
    let month = (doy / 30) + 1;
    let day   = (doy % 30) + 1;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hours, mins, secs)
}
