use leptos::prelude::*;
use leptos::task::spawn_local;

use std::collections::HashMap;
use std::collections::BTreeMap;
use crate::analytics::optimizer::{score_loadouts, score_loadouts_range};
use crate::domain::types::{CreatureConfig, LootItemConfig, Loadout, Maturity, SavedRun, Weapon, Amplifier};
use crate::persistence::idb::Db;

#[component]
pub fn AnalysePanel() -> impl IntoView {
    let (db, set_db) = signal_local::<Option<Db>>(None);
    let (runs,      set_runs)      = signal_local::<Vec<SavedRun>>(vec![]);
    let (creatures, set_creatures) = signal_local::<Vec<CreatureConfig>>(vec![]);
    let (loadouts,  set_loadouts)  = signal_local::<Vec<Loadout>>(vec![]);
    let (weapons,   set_weapons)   = signal_local::<Vec<Weapon>>(vec![]);
    let (amps,      set_amps)      = signal_local::<Vec<Amplifier>>(vec![]);
    let (mu_items, set_mu_items)   = signal_local::<Vec<LootItemConfig>>(vec![]);
    let ped_per_sp     = RwSignal::new(0.0_f64);
    // Skill-Referenz-Level: "Evader=450,Longblades=320" — für log. Prognose
    let skill_refs_str = RwSignal::new(String::new());

    let sel_creature   = RwSignal::new(String::new());
    let sel_maturity   = RwSignal::new(String::new());
    let range_mode     = RwSignal::new(false);
    let sel_mat_from   = RwSignal::new(String::new());
    let sel_mat_to     = RwSignal::new(String::new());
    let budget_kills   = RwSignal::new(100u32);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await { set_db.set(Some(d)); }
        });
    });

    Effect::new({
        let db = db.clone();
        move |_| {
            if let Some(d) = db.get() {
                let d2 = d.clone(); let d3 = d.clone(); let d4 = d.clone(); let d5 = d.clone();
                let d6 = d.clone(); let d7 = d.clone(); let d8 = d.clone();
                spawn_local(async move { if let Ok(v) = d.get_all_runs().await { set_runs.set(v); }});
                spawn_local(async move { if let Ok(v) = d2.get_all_creatures().await { set_creatures.set(v); }});
                spawn_local(async move { if let Ok(v) = d3.get_all_loadouts().await { set_loadouts.set(v); }});
                spawn_local(async move { if let Ok(v) = d4.get_all_weapons().await { set_weapons.set(v); }});
                spawn_local(async move { if let Ok(v) = d5.get_all_amps().await { set_amps.set(v); }});
                spawn_local(async move { if let Ok(v) = d6.get_all_loot_item_configs().await { set_mu_items.set(v); }});
                spawn_local(async move {
                    if let Ok(Some(v)) = d7.get_setting("ped_per_skillpoint").await {
                        if let Ok(f) = v.parse::<f64>() { ped_per_sp.set(f); }
                    }
                });
                spawn_local(async move {
                    if let Ok(Some(v)) = d8.get_setting("skill_levels").await {
                        skill_refs_str.set(v);
                    }
                });
            }
        }
    });

    // ── Referenz-Kosten/Kill ─────────────────────────────────────────────────
    // Wird aus den tatsächlichen Run-Daten der gewählten Kreatur berechnet.
    // Alle Schwellenwerte (Histogramm-Buckets, Streak-Threshold, Big-Loot) basieren darauf.
    let cost_per_kill_ref = Memo::new(move |_| {
        let sc = sel_creature.get();
        let all_runs = runs.get();
        let total_cost: f64 = all_runs.iter()
            .filter(|r| sc.is_empty() || r.config.creature == sc)
            .map(|r| r.total_cost_ped)
            .sum();
        let total_kills: u64 = all_runs.iter()
            .filter(|r| sc.is_empty() || r.config.creature == sc)
            .map(|r| r.stats.kills)
            .sum();
        if total_kills > 0 && total_cost > 0.0 {
            total_cost / total_kills as f64
        } else {
            // Fallback: versuche aus HP der Kreatur zu schätzen (0.01 PED pro HP als Näherung)
            // Falls auch keine Kreatur gewählt: 1.0 PED als Default
            let hp_guess = {
                let creature_list = all_runs.iter()
                    .filter(|r| !r.config.creature.is_empty() && (sc.is_empty() || r.config.creature == sc))
                    .count();
                let _ = creature_list;
                0.0_f64
            };
            if hp_guess > 0.0 { hp_guess } else { 1.0 }
        }
    });

    view! {
        <div class="space-y-6 max-w-4xl ">
            <h2 class="text-lg font-semibold ">"📈 Analyse"</h2>

            // Loadout-Optimizer
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Loadout-Optimizer"</h3>

                // Kreatur + Modus-Toggle
                <div class="flex gap-3 flex-wrap items-end text-sm ">
                    <label class="flex flex-col gap-1 ">
                        "Kreatur"
                        <select class="input " on:change=move |ev| {
                            sel_creature.set(event_target_value(&ev));
                            sel_maturity.set(String::new());
                            sel_mat_from.set(String::new());
                            sel_mat_to.set(String::new());
                        }>
                            <option value="">"- wählen -"</option>
                            {move || creatures.get().iter().map(|c| {
                                let name = c.creature.clone();
                                let name2 = name.clone();
                                view!{<option value=name>{name2}</option>}
                            }).collect_view()}
                        </select>
                    </label>

                    // Modus-Toggle
                    <div class="flex gap-1 text-xs pb-1 ">
                        <button
                            class=move || if !range_mode.get() {
                                "px-3 py-1 rounded bg-blue-600 text-white font-semibold"
                            } else {
                                "px-3 py-1 rounded bg-gray-100 text-gray-600 hover:bg-gray-200"
                            }
                            on:click=move |_| range_mode.set(false)
                        >"Einzeln"</button>
                        <button
                            class=move || if range_mode.get() {
                                "px-3 py-1 rounded bg-blue-600 text-white font-semibold"
                            } else {
                                "px-3 py-1 rounded bg-gray-100 text-gray-600 hover:bg-gray-200"
                            }
                            on:click=move |_| range_mode.set(true)
                        >"Range"</button>
                    </div>

                    // Einzeln-Auswahl
                    {move || (!range_mode.get()).then(|| view! {
                        <label class="flex flex-col gap-1 ">
                            "Maturity"
                            <select class="input "
                                on:change=move |ev| sel_maturity.set(event_target_value(&ev))>
                                <option value="">"- wählen -"</option>
                                {move || {
                                    let sc = sel_creature.get();
                                    creatures.get().iter()
                                        .find(|c| c.creature == sc)
                                        .map(|c| c.maturities.iter().map(|m| {
                                            let n = m.name.clone();
                                            view!{<option value=n.clone()>{n.clone()}</option>}
                                        }).collect_view())
                                }}
                            </select>
                        </label>
                    })}

                    // Range-Auswahl
                    {move || range_mode.get().then(|| view! {
                        <label class="flex flex-col gap-1 ">
                            "Von"
                            <select class="input "
                                on:change=move |ev| sel_mat_from.set(event_target_value(&ev))>
                                <option value="">"- von -"</option>
                                {move || {
                                    let sc = sel_creature.get();
                                    creatures.get().iter()
                                        .find(|c| c.creature == sc)
                                        .map(|c| c.maturities.iter().map(|m| {
                                            let n = m.name.clone();
                                            view!{<option value=n.clone()>{n.clone()}</option>}
                                        }).collect_view())
                                }}
                            </select>
                        </label>
                        <label class="flex flex-col gap-1 ">
                            "Bis"
                            <select class="input "
                                on:change=move |ev| sel_mat_to.set(event_target_value(&ev))>
                                <option value="">"- bis -"</option>
                                {move || {
                                    let sc = sel_creature.get();
                                    creatures.get().iter()
                                        .find(|c| c.creature == sc)
                                        .map(|c| c.maturities.iter().map(|m| {
                                            let n = m.name.clone();
                                            view!{<option value=n.clone()>{n.clone()}</option>}
                                        }).collect_view())
                                }}
                            </select>
                        </label>
                    })}
                </div>

                // ── Einzeln-Ergebnis ─────────────────────────────────────────
                {move || {
                    if range_mode.get() { return view!{<div></div>}.into_any(); }
                    let sc = sel_creature.get();
                    let sm = sel_maturity.get();
                    if sc.is_empty() || sm.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Kreatur und Maturity wählen."</p>}.into_any();
                    }
                    let maturity = creatures.get().iter()
                        .find(|c| c.creature == sc)
                        .and_then(|c| c.maturities.iter().find(|m| m.name == sm))
                        .cloned();
                    let maturity = match maturity {
                        Some(m) => m,
                        None => return view!{<div></div>}.into_any(),
                    };
                    let ls_vec = loadouts.get();
                    let ws_vec = weapons.get();
                    let amps_vec = amps.get();
                    let combos: Vec<(Loadout, Weapon, Option<Amplifier>)> = ls_vec.iter().filter_map(|l| {
                        let w = ws_vec.iter().find(|w| w.id == l.weapon_id)?.clone();
                        let a = l.amp_id.as_ref().and_then(|aid| amps_vec.iter().find(|a| a.id == *aid)).cloned();
                        Some((l.clone(), w, a))
                    }).collect();
                    if combos.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Loadouts konfiguriert."</p>}.into_any();
                    }
                    let mut scores = score_loadouts(&maturity, &combos);
                    scores.sort_by(|a, b| a.cost_per_kill_ped.partial_cmp(&b.cost_per_kill_ped).unwrap_or(std::cmp::Ordering::Equal));
                    let best_id = scores.first().map(|s| s.loadout_id.clone()).unwrap_or_default();
                    view! {
                        <table class="w-full text-sm border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-2 ">"Loadout"</th>
                                    <th class="text-right p-2 ">"Ø Dmg"</th>
                                    <th class="text-right p-2 ">"PEC/Shot"</th>
                                    <th class="text-right p-2 ">"Schüsse/Kill"</th>
                                    <th class="text-right p-2 ">"Ø Overkill"</th>
                                    <th class="text-right p-2 ">"Kosten/Kill"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {scores.into_iter().map(|s| {
                                    let is_best = s.loadout_id == best_id;
                                    view! {
                                        <tr class=if is_best {"bg-green-50 font-semibold border-b"} else {"border-b"}>
                                            <td class="p-2 ">{s.loadout_name.clone()} {if is_best {" ✅"} else {""}}</td>
                                            <td class="text-right p-2 ">{format!("{:.1}", s.avg_damage)}</td>
                                            <td class="text-right p-2 ">{format!("{:.2}", s.pec_per_shot)}</td>
                                            <td class="text-right p-2 ">{s.shots_per_kill}</td>
                                            <td class="text-right p-2 ">{format!("{:.1} pts", s.avg_overkill_pts)}</td>
                                            <td class="text-right p-2 ">{format!("{:.4} PED", s.cost_per_kill_ped)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}

                // ── Range-Ergebnis ───────────────────────────────────────────
                {move || {
                    if !range_mode.get() { return view!{<div></div>}.into_any(); }
                    let sc   = sel_creature.get();
                    let from = sel_mat_from.get();
                    let to   = sel_mat_to.get();
                    if sc.is_empty() || from.is_empty() || to.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Kreatur und Maturity-Range wählen."</p>}.into_any();
                    }

                    // Maturities der Kreatur in ihrer definierten Reihenfolge
                    let all_mats: Vec<_> = creatures.get().iter()
                        .find(|c| c.creature == sc)
                        .map(|c| c.maturities.clone())
                        .unwrap_or_default();

                    let idx_from = all_mats.iter().position(|m| m.name == from);
                    let idx_to   = all_mats.iter().position(|m| m.name == to);

                    let (idx_start, idx_end) = match (idx_from, idx_to) {
                        (Some(a), Some(b)) => (a.min(b), a.max(b)),
                        _ => return view!{<p class="text-red-400 text-sm ">"Maturity nicht gefunden."</p>}.into_any(),
                    };

                    let range_mats: Vec<&Maturity> = all_mats[idx_start..=idx_end].iter().collect();
                    let mat_label = format!("{} → {}", all_mats[idx_start].name, all_mats[idx_end].name);
                    let mat_count = range_mats.len();

                    let ls_vec = loadouts.get();
                    let ws_vec = weapons.get();
                    let amps_vec = amps.get();
                    let combos: Vec<(Loadout, Weapon, Option<Amplifier>)> = ls_vec.iter().filter_map(|l| {
                        let w = ws_vec.iter().find(|w| w.id == l.weapon_id)?.clone();
                        let a = l.amp_id.as_ref().and_then(|aid| amps_vec.iter().find(|a| a.id == *aid)).cloned();
                        Some((l.clone(), w, a))
                    }).collect();
                    if combos.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Loadouts konfiguriert."</p>}.into_any();
                    }

                    let mut scores = score_loadouts_range(&range_mats, &combos);
                    scores.sort_by(|a, b| a.avg_cost_per_kill.partial_cmp(&b.avg_cost_per_kill)
                        .unwrap_or(std::cmp::Ordering::Equal));
                    let best_id = scores.first().map(|s| s.loadout_id.clone()).unwrap_or_default();

                    view! {
                        <div class="space-y-2 ">
                            <div class="text-xs text-gray-500 ">
                                "Range: " <span class="font-semibold text-gray-700 ">{mat_label}</span>
                                " (" {mat_count} " Maturities, gleichgewichtet)"
                            </div>
                            <table class="w-full text-sm border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-2 ">"Loadout"</th>
                                        <th class="text-right p-2 ">"Ø Dmg"</th>
                                        <th class="text-right p-2 ">"PEC/Shot"</th>
                                        <th class="text-right p-2 ">"Ø Shots/Kill"</th>
                                        <th class="text-right p-2 ">"Ø Overkill"</th>
                                        <th class="text-right p-2 ">"Ø Kosten/Kill"</th>
                                        <th class="text-right p-2 ">"Worst-case"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {scores.into_iter().map(|s| {
                                        let is_best = s.loadout_id == best_id;
                                        // Detail-Zeilen pro Maturity (aufklappbar per title-Tooltip)
                                        let detail = s.per_maturity.iter()
                                            .map(|(name, sc)| format!("{}: {:.4} PED/Kill", name, sc.cost_per_kill_ped))
                                            .collect::<Vec<_>>()
                                            .join(" | ");
                                        view! {
                                            <tr class=if is_best {"bg-green-50 font-semibold border-b"} else {"border-b"}
                                                title=detail>
                                                <td class="p-2 ">{s.loadout_name.clone()} {if is_best {" ✅"} else {""}}</td>
                                                <td class="text-right p-2 ">{format!("{:.1}", s.avg_damage)}</td>
                                                <td class="text-right p-2 ">{format!("{:.2}", s.pec_per_shot)}</td>
                                                <td class="text-right p-2 ">{format!("{:.1}", s.avg_shots_per_kill)}</td>
                                                <td class="text-right p-2 ">{format!("{:.1} pts", s.avg_overkill_pts)}</td>
                                                <td class="text-right p-2 ">{format!("{:.4} PED", s.avg_cost_per_kill)}</td>
                                                <td class="text-right p-2 text-orange-600 ">{format!("{:.4} PED", s.max_cost_per_kill)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            <p class="text-xs text-gray-400 italic ">
                                "💡 Hover über eine Zeile für Kosten pro Maturity."
                            </p>
                        </div>
                    }.into_any()
                }}

                // ── Feature 8: Loadout-Kosten-Optimierung ─────────────────────
                {move || {
                    let sc = sel_creature.get();
                    if sc.is_empty() { return view!{<span></span>}.into_any(); }
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() { return view!{<span></span>}.into_any(); }

                    let total_kills: u64 = filtered.iter().map(|r| r.stats.kills).sum();
                    let total_loot: f64  = filtered.iter().map(|r| r.stats.total_loot_value_ped).sum();
                    let total_cost: f64  = filtered.iter().map(|r| r.total_cost_ped).sum();
                    if total_kills == 0 { return view!{<span></span>}.into_any(); }

                    let avg_loot_per_kill = total_loot / total_kills as f64;
                    let avg_cost_per_kill = total_cost / total_kills as f64;
                    let overhead_budget   = avg_loot_per_kill - avg_cost_per_kill;
                    let break_even_mu = if total_cost > 0.0 { total_loot / total_cost * 100.0 } else { 0.0 };
                    let overhead_cls = if overhead_budget >= 0.0 { "text-green-700 font-semibold" } else { "text-red-600 font-semibold" };

                    view! {
                        <div class="mt-3 bg-blue-50 border border-blue-100 rounded p-3 text-sm space-y-1 ">
                            <div class="font-medium text-blue-800 ">"💰 Zusatzkosten-Budget (aus Run-Daten)"</div>
                            <div class="grid grid-cols-2 gap-x-4 text-xs ">
                                <span class="text-gray-600 ">"Ø Loot/Kill:"</span>
                                <span class="font-mono ">{format!("{:.4} PED", avg_loot_per_kill)}</span>
                                <span class="text-gray-600 ">"Ø Gesamtkosten/Kill:"</span>
                                <span class="font-mono ">{format!("{:.4} PED", avg_cost_per_kill)}</span>
                                <span class="text-gray-600 ">"Budget für Rüstung/FAP/Kill:"</span>
                                <span class=format!("font-mono {}", overhead_cls)>{format!("{:+.4} PED", overhead_budget)}</span>
                                <span class="text-gray-600 ">"Break-Even MU:"</span>
                                <span class="font-mono ">{format!("{:.1}%", break_even_mu)}</span>
                            </div>
                            <p class="text-xs text-gray-400 italic ">
                                "Basis: " {total_kills} " Kills. Rüstungs-Decay sollte unter "
                                {format!("{:.4}", overhead_budget.max(0.0))}
                                " PED/Kill bleiben."
                            </p>
                        </div>
                    }.into_any()
                }}
            </section>

            // Loot-Histogramm
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Loot-Histogramm"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let kill_loots: Vec<f64> = runs.get().iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .flat_map(|r| r.stats.kill_loots.iter().map(|k| k.value_ped))
                        .collect();

                    if kill_loots.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    // Dynamische Buckets relativ zu Ø Kosten/Kill
                    let cpk = cost_per_kill_ref.get();
                    let b = [cpk * 0.5, cpk * 1.0, cpk * 2.0, cpk * 5.0];
                    let buckets: Vec<(f64, f64, String)> = vec![
                        (0.0,  b[0], format!("< {:.3} PED",        b[0])),
                        (b[0], b[1], format!("{:.3}–{:.3} PED",    b[0], b[1])),
                        (b[1], b[2], format!("{:.3}–{:.3} PED",    b[1], b[2])),
                        (b[2], b[3], format!("{:.3}–{:.3} PED",    b[2], b[3])),
                        (b[3], f64::MAX, format!("{:.3}+ PED",     b[3])),
                    ];
                    let total = kill_loots.len() as f64;
                    let counts: Vec<(String, usize, bool)> = buckets.iter().map(|(lo, hi, label)| {
                        let cnt = kill_loots.iter().filter(|&&v| v >= *lo && v < *hi).count();
                        // Bucket [1×cpk, 2×cpk) = Break-Even-Zone (grün hervorheben)
                        let is_breakeven = (*lo - cpk).abs() < 0.0001;
                        (label.clone(), cnt, is_breakeven)
                    }).collect();
                    let max_cnt = counts.iter().map(|(_, c, _)| *c).max().unwrap_or(1) as f64;

                    view! {
                        <div class="space-y-1 text-sm font-mono ">
                            <div class="text-xs text-gray-400 mb-2 ">
                                "Referenz: " {format!("{:.4} PED/Kill", cpk)}
                                " · Buckets = ×0.5 / ×1 / ×2 / ×5 des Ø Kill-Preises"
                            </div>
                            {counts.into_iter().map(|(label, cnt, is_be)| {
                                let bar_w = (cnt as f64 / max_cnt * 200.0) as u32;
                                let bar_cls = if is_be { "bg-green-400 h-4 rounded" } else { "bg-blue-400 h-4 rounded" };
                                view! {
                                    <div class="flex items-center gap-2 ">
                                        <span class="w-36 text-right text-xs ">
                                            {label}
                                            {if is_be { " ✓" } else { "" }}
                                        </span>
                                        <div class=bar_cls style=format!("width: {}px", bar_w)></div>
                                        <span class="text-gray-600 ">{format!("{} Kills ({:.0}%)", cnt, cnt as f64/total*100.0)}</span>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Maturity-Statistik ────────────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🎯 Kills & Loot pro Maturity"</h3>
                {move || {
                    let sc = sel_creature.get();
                    if sc.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Kreatur wählen."</p>}.into_any();
                    }

                    // Alle kill_loots mit Maturity-Info aus gefilterten Runs sammeln
                    // Struktur: maturity_name → (kills, total_loot)
                    let mut mat_stats: std::collections::BTreeMap<String, (u64, f64)> = std::collections::BTreeMap::new();
                    let mut unknown_kills: u64 = 0;
                    let mut unknown_loot:  f64 = 0.0;

                    for run in runs.get().iter().filter(|r| r.config.creature == sc) {
                        for kl in &run.stats.kill_loots {
                            match &kl.maturity {
                                Some(m) => {
                                    let e = mat_stats.entry(m.clone()).or_insert((0, 0.0));
                                    e.0 += 1;
                                    e.1 += kl.value_ped;
                                }
                                None => {
                                    unknown_kills += 1;
                                    unknown_loot  += kl.value_ped;
                                }
                            }
                        }
                    }

                    let total_kills: u64 = mat_stats.values().map(|(k, _)| k).sum::<u64>() + unknown_kills;
                    if total_kills == 0 {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Kill-Daten."</p>}.into_any();
                    }

                    // Ø Loot über alle bekannten Maturities (für Vergleich)
                    let known_kills: u64 = mat_stats.values().map(|(k, _)| *k).sum();
                    let known_loot:  f64 = mat_stats.values().map(|(_, l)| *l).sum();
                    let global_avg = if known_kills > 0 { known_loot / known_kills as f64 } else { 0.0 };

                    // Kreatur-Maturity-Reihenfolge aus Konfiguration holen (für Sortierung)
                    let configured_order: Vec<String> = creatures.get().iter()
                        .find(|c| c.creature == sc)
                        .map(|c| c.maturities.iter().map(|m| m.name.clone()).collect())
                        .unwrap_or_default();

                    // Zeilen: erst konfigurierte Maturities in Reihenfolge, dann unbekannte
                    let mut rows: Vec<(String, u64, f64)> = configured_order.iter()
                        .filter_map(|name| {
                            mat_stats.get(name).map(|(k, l)| (name.clone(), *k, *l))
                        })
                        .collect();
                    // Maturities die nicht in der Konfiguration stehen (trotzdem gejagt)
                    for (name, (k, l)) in &mat_stats {
                        if !configured_order.contains(name) {
                            rows.push((name.clone(), *k, *l));
                        }
                    }

                    view! {
                        <div class="space-y-2 ">
                            <table class="w-full text-sm border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-2 ">"Maturity"</th>
                                        <th class="text-right p-2 ">"Kills"</th>
                                        <th class="text-right p-2 ">"Anteil"</th>
                                        <th class="text-right p-2 ">"Loot gesamt"</th>
                                        <th class="text-right p-2 ">"Ø Loot/Kill"</th>
                                        <th class="text-right p-2 ">"vs. Ø"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|(name, kills, loot)| {
                                        let avg = if kills > 0 { loot / kills as f64 } else { 0.0 };
                                        let pct_kills = kills as f64 / total_kills as f64 * 100.0;
                                        let vs_avg = avg - global_avg;
                                        let vs_cls = if vs_avg > global_avg * 0.1 {
                                            "text-right p-2 text-green-600 font-semibold"
                                        } else if vs_avg < -(global_avg * 0.1) {
                                            "text-right p-2 text-red-500"
                                        } else {
                                            "text-right p-2 text-gray-500"
                                        };
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-2 font-medium ">{name}</td>
                                                <td class="text-right p-2 ">{kills}</td>
                                                <td class="text-right p-2 text-gray-500 ">{format!("{:.0}%", pct_kills)}</td>
                                                <td class="text-right p-2 ">{format!("{:.3} PED", loot)}</td>
                                                <td class="text-right p-2 ">{format!("{:.4} PED", avg)}</td>
                                                <td class=vs_cls>{format!("{:+.4} PED", vs_avg)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                    {(unknown_kills > 0).then(|| view! {
                                        <tr class="border-b text-gray-400 italic ">
                                            <td class="p-2 ">"(unbekannt)"</td>
                                            <td class="text-right p-2 ">{unknown_kills}</td>
                                            <td class="text-right p-2 ">{format!("{:.0}%", unknown_kills as f64 / total_kills as f64 * 100.0)}</td>
                                            <td class="text-right p-2 ">{format!("{:.3} PED", unknown_loot)}</td>
                                            <td class="text-right p-2 ">{format!("{:.4} PED", if unknown_kills > 0 { unknown_loot / unknown_kills as f64 } else { 0.0 })}</td>
                                            <td class="text-right p-2 ">"-"</td>
                                        </tr>
                                    })}
                                </tbody>
                                <tfoot class="border-t bg-gray-50 font-semibold ">
                                    <tr>
                                        <td class="p-2 ">"Gesamt"</td>
                                        <td class="text-right p-2 ">{total_kills}</td>
                                        <td class="text-right p-2 ">"100%"</td>
                                        <td class="text-right p-2 ">{format!("{:.3} PED", known_loot + unknown_loot)}</td>
                                        <td class="text-right p-2 ">{format!("{:.4} PED", global_avg)}</td>
                                        <td class="text-right p-2 ">"-"</td>
                                    </tr>
                                </tfoot>
                            </table>
                            <p class="text-xs text-gray-400 italic ">
                                "\"vs. Ø\" = Abweichung vom Durchschnitt aller Maturities. "
                                "Grün = >10% über Ø, Rot = >10% darunter. "
                                "Unbekannte Kills = Kreatur ohne Maturity-Konfiguration oder HP außerhalb der definierten Ranges."
                            </p>
                        </div>
                    }.into_any()
                }}
            </section>

            // Kills/Stunde & Survival
            <section class="space-y-2 ">
                <h3 class="font-medium ">"Kills/Stunde & Survival"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let filtered: Vec<_> = runs.get().into_iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let total_kills: u64 = filtered.iter().map(|r| r.stats.kills).sum();
                    let total_secs: u64  = filtered.iter().map(|r| r.stopped_at.saturating_sub(r.started_at)).sum();
                    let kills_h = if total_secs > 0 { total_kills as f64 / (total_secs as f64 / 3600.0) } else { 0.0 };
                    let total_evades: u64 = filtered.iter().map(|r| r.stats.player_evades).sum();
                    let total_hits:   u64 = filtered.iter().map(|r| r.stats.player_hits + r.stats.player_crit_hits).sum();
                    let dodge_rate = if (total_hits + total_evades) > 0 {
                        total_evades as f64 / (total_hits + total_evades) as f64 * 100.0
                    } else { 0.0 };

                    let avg_kill_secs = if total_kills > 0 { total_secs as f64 / total_kills as f64 } else { 0.0 };
                    let max_gap_secs: u64 = filtered.iter().flat_map(|r| {
                        r.stats.kill_timestamps.windows(2).map(|w| w[1].saturating_sub(w[0]))
                    }).max().unwrap_or(0);
                    view! {
                        <div class="text-sm space-y-1 ">
                            <div>"Kills/h: " <strong>{format!("{:.1}", kills_h)}</strong></div>
                            <div>"Dodge-Rate: " <strong>{format!("{:.1}%", dodge_rate)}</strong></div>
                            <div>"Ø Kill-Zeit: " <strong>{format!("{:.0}s", avg_kill_secs)}</strong></div>
                            <div>"Max. Kill-Gap: " <strong>{format!("{}s", max_gap_secs)}</strong></div>
                        </div>
                    }.into_any()
                }}
            </section>

            // Tageszeit-Analyse
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Tageszeit-Analyse"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let mut hour_kills   = [0u64; 24];
                    let mut hour_cost    = [0f64; 24];
                    let mut hour_loot_tt = [0f64; 24];
                    for r in &filtered {
                        let hour = ((r.started_at % 86400) / 3600) as usize;
                        let h = hour.min(23);
                        hour_kills[h]   += r.stats.kills;
                        hour_cost[h]    += r.total_cost_ped;
                        hour_loot_tt[h] += r.stats.total_loot_value_ped;
                    }
                    let max_kills = *hour_kills.iter().max().unwrap_or(&1).max(&1);
                    view! {
                        <div class="space-y-1 text-xs font-mono ">
                            {(0..24usize).map(|h| {
                                let kills = hour_kills[h];
                                let bar_w = (kills as f64 / max_kills as f64 * 150.0) as u32;
                                let ret = if hour_cost[h] > 0.0 { hour_loot_tt[h] / hour_cost[h] * 100.0 } else { 0.0 };
                                let ret_cls = if ret >= 100.0 { "text-green-600" } else if ret > 0.0 { "text-red-500" } else { "text-gray-400" };
                                view! {
                                    <div class="flex items-center gap-2 ">
                                        <span class="w-6 text-right text-gray-500 ">{format!("{:02}", h)}</span>
                                        <div class="bg-blue-400 h-3 rounded " style=format!("width:{}px;min-width:{}px", bar_w, if kills > 0 {2} else {0})></div>
                                        <span class="text-gray-500 ">{kills} " kills"</span>
                                        {if kills > 0 { Some(view!{
                                            <span class=ret_cls>{format!("| {:.0}% TT", ret)}</span>
                                        })} else { None }}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            // Skill-Gain-Analyse
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Skill-Gains pro Kreatur"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let total_secs: u64 = filtered.iter().map(|r| r.stopped_at.saturating_sub(r.started_at)).sum();
                    let mut skill_totals: HashMap<String, f64> = HashMap::new();
                    for r in &filtered {
                        for (skill, &amount) in &r.stats.skill_gains {
                            *skill_totals.entry(skill.clone()).or_insert(0.0) += amount;
                        }
                    }
                    if skill_totals.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Skill-Gains erfasst."</p>}.into_any();
                    }
                    let hours = total_secs as f64 / 3600.0;
                    let mut skills: Vec<(String, f64)> = skill_totals.into_iter().collect();
                    skills.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let pps = ped_per_sp.get();
                    let total_skill_value: f64 = skills.iter().map(|(_, t)| t * pps).sum();
                    view! {
                        <table class="w-full text-xs border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-1.5 ">"Skill"</th>
                                    <th class="text-right p-1.5 ">"Gesamt"</th>
                                    <th class="text-right p-1.5 ">"pro Stunde"</th>
                                    {if pps > 0.0 { Some(view!{<th class="text-right p-1.5 ">"PED-Wert"</th>}) } else { None }}
                                </tr>
                            </thead>
                            <tbody>
                                {skills.into_iter().map(|(skill, total)| {
                                    let per_h = if hours > 0.0 { total / hours } else { 0.0 };
                                    let ped_val = total * pps;
                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-1.5 ">{skill}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.4}", total)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.4}/h", per_h)}</td>
                                            {if pps > 0.0 { Some(view!{<td class="text-right p-1.5 text-green-700 ">{format!("{:.4} PED", ped_val)}</td>}) } else { None }}
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                            {if pps > 0.0 { Some(view!{
                                <tfoot>
                                    <tr class="border-t font-semibold bg-gray-50 ">
                                        <td class="p-1.5 " colspan="3">"Gesamt Skill-Wert"</td>
                                        <td class="text-right p-1.5 text-green-700 ">{format!("{:.4} PED", total_skill_value)}</td>
                                    </tr>
                                </tfoot>
                            })} else { None }}
                        </table>
                        {if pps <= 0.0 { Some(view!{
                            <p class="text-xs text-gray-400 italic ">"PED/Skill-Punkt in Einstellungen konfigurieren für Wertanzeige."</p>
                        })} else { None }}
                    }.into_any()
                }}
            </section>

            // Loot-Streak & Trockenphase
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Loot-Streak & Trockenphase"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let all_loots: Vec<f64> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .flat_map(|r| r.stats.kill_loots.iter().map(|k| k.value_ped))
                        .collect();
                    if all_loots.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Kill-Loot-Daten."</p>}.into_any();
                    }
                    // Schwellenwert = Ø Kosten/Kill (Break-Even-Linie)
                    let threshold = cost_per_kill_ref.get();
                    let mut max_dry = 0usize;
                    let mut cur_dry = 0usize;
                    let mut max_streak = 0usize;
                    let mut cur_streak = 0usize;
                    for (i, &v) in all_loots.iter().enumerate() {
                        if v >= threshold {
                            cur_streak += 1;
                            if cur_streak > max_streak { max_streak = cur_streak; }
                            if cur_dry > max_dry { max_dry = cur_dry; }
                            cur_dry = 0;
                        } else {
                            cur_dry += 1;
                            cur_streak = 0;
                        }
                        let _ = i;
                    }
                    if cur_dry > max_dry { max_dry = cur_dry; }
                    let total = all_loots.len();
                    let good = all_loots.iter().filter(|&&v| v >= threshold).count();
                    let current_dry: usize = all_loots.iter().rev().take_while(|&&v| v < threshold).count();
                    view! {
                        <div class="text-sm space-y-2 ">
                            <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 ">
                                <div class="bg-white border rounded p-2 text-center ">
                                    <div class="text-lg font-bold ">{current_dry}</div>
                                    <div class="text-xs text-gray-500 ">"Akt. Dry-Streak"</div>
                                </div>
                                <div class="bg-white border rounded p-2 text-center ">
                                    <div class="text-lg font-bold text-red-500 ">{max_dry}</div>
                                    <div class="text-xs text-gray-500 ">"Längste Trockenphase"</div>
                                </div>
                                <div class="bg-white border rounded p-2 text-center ">
                                    <div class="text-lg font-bold text-green-600 ">{max_streak}</div>
                                    <div class="text-xs text-gray-500 ">"Längste Glücks-Serie"</div>
                                </div>
                                <div class="bg-white border rounded p-2 text-center ">
                                    <div class="text-lg font-bold ">{format!("{:.0}%", good as f64 / total as f64 * 100.0)}</div>
                                    <div class="text-xs text-gray-500 ">"Kills ≥ Break-Even"</div>
                                </div>
                            </div>
                            <div class="text-xs text-gray-400 ">
                                "Schwellwert (Break-Even): " {format!("{:.4} PED/Kill", threshold)}
                            </div>
                        </div>
                    }.into_any()
                }}
            </section>

            // Drop-Statistiken per Kreatur
            <section class="space-y-3 ">
                <h3 class="font-medium ">"Drop-Statistiken per Kreatur"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();

                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    // Aggregiere Drop-Stats: item -> (kills_with_drop, total_qty, total_tt)
                    #[derive(Default)]
                    struct ItemStats {
                        kills_with_drop: u64,
                        total_qty: u64,
                        total_tt: f64,
                    }

                    let total_kills: u64 = filtered.iter().map(|r| r.stats.kills).sum();
                    let mut item_map: HashMap<String, ItemStats> = HashMap::new();

                    for r in &filtered {
                        for kl in &r.stats.kill_loots {
                            // Pro Kill schauen welche Items gelooted wurden
                            // Approximation: wir zählen Loot-Events per Item vs Kills
                            let _ = kl; // kill_loots enthalten keine Item-Aufschlüsselung
                        }
                        // Direkte Item-Aggregation
                        for (name, agg) in &r.stats.loot_items {
                            let e = item_map.entry(name.clone()).or_default();
                            e.kills_with_drop += agg.event_count;
                            e.total_qty       += agg.total_qty;
                            e.total_tt        += agg.total_value_ped;
                        }
                    }

                    let mut items: Vec<(String, ItemStats)> = item_map.into_iter().collect();
                    // Sortiere nach Drop-Häufigkeit (events/kill absteigend)
                    items.sort_by(|a, b| {
                        let ra = a.1.kills_with_drop as f64 / total_kills.max(1) as f64;
                        let rb = b.1.kills_with_drop as f64 / total_kills.max(1) as f64;
                        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    view! {
                        <div>
                            <div class="text-xs text-gray-500 mb-2 ">"Basis: " {total_kills} " Kills"
                                {if !sc.is_empty() { format!(" von {}", sc) } else { " (alle Kreaturen)".to_string() }}
                            </div>
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Item"</th>
                                        <th class="text-right p-1.5 ">"Drop-Events"</th>
                                        <th class="text-right p-1.5 ">"Events/Kill"</th>
                                        <th class="text-right p-1.5 ">"Ø Qty/Kill"</th>
                                        <th class="text-right p-1.5 ">"Ø TT/Kill"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|(name, stat)| {
                                        let events_per_kill = stat.kills_with_drop as f64 / total_kills.max(1) as f64;
                                        let qty_per_kill    = stat.total_qty as f64 / total_kills.max(1) as f64;
                                        let tt_per_kill     = stat.total_tt / total_kills.max(1) as f64;
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-1.5 ">{name}</td>
                                                <td class="text-right p-1.5 ">{stat.kills_with_drop}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.3}", events_per_kill)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.2}", qty_per_kill)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.4} PED", tt_per_kill)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Break-Even-MU-Rechner ─────────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"💹 Break-Even-MU-Rechner"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let avg_return_tt = stat_mean(&filtered.iter().map(|r| r.return_pct_tt).collect::<Vec<_>>());
                    let break_even_mu = if avg_return_tt > 0.0 { 10000.0 / avg_return_tt } else { 0.0 };
                    let mu_cls = if break_even_mu <= 110.0 { "text-green-600 font-bold" }
                                 else if break_even_mu <= 140.0 { "text-yellow-600 font-bold" }
                                 else { "text-red-500 font-bold" };

                    // Last 5 runs newest first
                    let last5: Vec<&SavedRun> = filtered.iter().rev().take(5).copied().collect();

                    view! {
                        <div class="space-y-3 ">
                            <div class="text-sm space-y-1 ">
                                <div>"Ø TT-Return: " <strong>{format!("{:.1}%", avg_return_tt)}</strong></div>
                                <div>"Break-Even MU: " <span class=mu_cls>{format!("{:.1}%", break_even_mu)}</span></div>
                                <div class="text-xs text-gray-500 ">
                                    {format!("D.h. falls du Loot für durchschnittlich {:.1}% MU verkaufst, deckst du deine Kosten.", break_even_mu)}
                                </div>
                            </div>
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Datum"</th>
                                        <th class="text-right p-1.5 ">"TT-Return"</th>
                                        <th class="text-right p-1.5 ">"Benötigtes MU"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {last5.into_iter().map(|r| {
                                        let date = format_ts(r.started_at);
                                        let ret = r.return_pct_tt;
                                        let needed_mu = if ret > 0.0 { 10000.0 / ret } else { 0.0 };
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-1.5 ">{date}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", ret)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", needed_mu)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── MU-Impact-Analyse ──────────────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"📊 MU-Impact-Analyse"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let mu_cfg = mu_items.get();
                    let mu_cfg_map: HashMap<String, f64> = mu_cfg.iter().map(|c| (c.name.clone(), c.mu_pct)).collect();

                    // Aggregate item TT totals
                    let mut item_tt: HashMap<String, f64> = HashMap::new();
                    for r in &filtered {
                        for (name, agg) in &r.stats.loot_items {
                            *item_tt.entry(name.clone()).or_insert(0.0) += agg.total_value_ped;
                        }
                    }

                    // Build item analysis sorted by mu_gain desc
                    let mut items: Vec<(String, f64, f64, f64, f64)> = item_tt.into_iter().map(|(name, total_tt)| {
                        let mu_pct = *mu_cfg_map.get(&name).unwrap_or(&100.0);
                        let mu_value = total_tt * mu_pct / 100.0;
                        let mu_gain = total_tt * (mu_pct / 100.0 - 1.0);
                        (name, total_tt, mu_pct, mu_value, mu_gain)
                    }).collect();
                    items.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));

                    let total_tt: f64  = items.iter().map(|i| i.1).sum();
                    let total_mu: f64  = items.iter().map(|i| i.3).sum();
                    let total_gain: f64 = items.iter().map(|i| i.4).sum();

                    view! {
                        <div class="space-y-2 ">
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Item"</th>
                                        <th class="text-right p-1.5 ">"TT-Wert"</th>
                                        <th class="text-right p-1.5 ">"MU%"</th>
                                        <th class="text-right p-1.5 ">"MU-Wert"</th>
                                        <th class="text-right p-1.5 ">"Gain vs TT"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|(name, tt, mu_pct, mu_val, gain)| {
                                        let row_cls = if mu_pct >= 200.0 { "border-b bg-green-50 font-bold text-green-700" }
                                                      else if mu_pct >= 150.0 { "border-b bg-green-50 text-green-700" }
                                                      else { "border-b hover:bg-gray-50" };
                                        view! {
                                            <tr class=row_cls>
                                                <td class="p-1.5 ">{name}</td>
                                                <td class="text-right p-1.5 font-mono ">{format!("{:.4}", tt)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", mu_pct)}</td>
                                                <td class="text-right p-1.5 font-mono ">{format!("{:.4}", mu_val)}</td>
                                                <td class="text-right p-1.5 font-mono ">{format!("{:+.4}", gain)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                    <tr class="border-t-2 font-semibold bg-gray-100 ">
                                        <td class="p-1.5 ">"Gesamt"</td>
                                        <td class="text-right p-1.5 font-mono ">{format!("{:.4}", total_tt)}</td>
                                        <td class="text-right p-1.5 ">"-"</td>
                                        <td class="text-right p-1.5 font-mono ">{format!("{:.4}", total_mu)}</td>
                                        <td class="text-right p-1.5 font-mono ">{format!("{:+.4}", total_gain)}</td>
                                    </tr>
                                </tbody>
                            </table>
                            <p class="text-xs text-gray-400 italic ">"MU% aus Konfiguration. Standard 100% wenn nicht gesetzt."</p>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Kreatur-Rentabilitäts-Score ─────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🏆 Kreatur-Rentabilitäts-Score"</h3>
                {move || {
                    let all_runs = runs.get();
                    if all_runs.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    // Group runs by creature
                    let mut by_creature: BTreeMap<String, Vec<&SavedRun>> = BTreeMap::new();
                    for r in &all_runs {
                        by_creature.entry(r.config.creature.clone()).or_default().push(r);
                    }

                    struct CreatureScore {
                        name: String,
                        avg_return_tt: f64,
                        kills_h: f64,
                        cv: f64,
                        score: f64,
                    }

                    let mut scores: Vec<CreatureScore> = by_creature.into_iter().filter_map(|(name, crs)| {
                        let total_kills: u64 = crs.iter().map(|r| r.stats.kills).sum();
                        if total_kills == 0 { return None; }
                        let total_secs: u64 = crs.iter().map(|r| r.stopped_at.saturating_sub(r.started_at)).sum();
                        let kills_h = if total_secs > 0 { total_kills as f64 / (total_secs as f64 / 3600.0) } else { 0.0 };
                        let returns: Vec<f64> = crs.iter().map(|r| r.return_pct_tt).collect();
                        let avg_return_tt = stat_mean(&returns);
                        let all_loots: Vec<f64> = crs.iter().flat_map(|r| r.stats.kill_loots.iter().map(|k| k.value_ped)).collect();
                        let loot_mean = stat_mean(&all_loots);
                        let loot_std  = stat_std(&all_loots);
                        let cv = if loot_mean > 0.0 { loot_std / loot_mean } else { 0.0 };
                        let score = avg_return_tt * kills_h.sqrt() / (1.0 + cv);
                        Some(CreatureScore { name, avg_return_tt, kills_h, cv, score })
                    }).collect();

                    scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                    let best_name = scores.first().map(|s| s.name.clone()).unwrap_or_default();

                    if scores.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    view! {
                        <div class="space-y-2 ">
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"#"</th>
                                        <th class="text-left p-1.5 ">"Kreatur"</th>
                                        <th class="text-right p-1.5 ">"Return TT"</th>
                                        <th class="text-right p-1.5 ">"Kills/h"</th>
                                        <th class="text-right p-1.5 ">"CV"</th>
                                        <th class="text-right p-1.5 ">"Score"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {scores.into_iter().enumerate().map(|(i, s)| {
                                        let is_best = s.name == best_name;
                                        let row_cls = if is_best { "border-b bg-green-50 font-semibold text-green-800" } else { "border-b hover:bg-gray-50" };
                                        view! {
                                            <tr class=row_cls>
                                                <td class="p-1.5 ">{i + 1}</td>
                                                <td class="p-1.5 ">{s.name} {if is_best {" 🏆"} else {""}}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", s.avg_return_tt)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}", s.kills_h)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.2}", s.cv)}</td>
                                                <td class="text-right p-1.5 font-mono ">{format!("{:.2}", s.score)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            <p class="text-xs text-gray-400 italic ">"Score = Return × √(Kills/h) / (1+CV). Höhere Kills/h und niedrige Varianz = besser."</p>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Event-Bonus-Tracking ───────────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🎉 Event-Bonus-Tracking"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    let event_runs: Vec<&&SavedRun> = filtered.iter().filter(|r| r.config.event_active).collect();
                    let normal_runs: Vec<&&SavedRun> = filtered.iter().filter(|r| !r.config.event_active).collect();

                    if event_runs.is_empty() {
                        return view!{
                            <p class="text-gray-400 text-sm italic ">"Noch keine Event-Runs markiert. Event-Checkbox beim Run-Start nutzen."</p>
                        }.into_any();
                    }

                    let avg_ret_event  = stat_mean(&event_runs.iter().map(|r| r.return_pct_tt).collect::<Vec<_>>());
                    let avg_prof_event = stat_mean(&event_runs.iter().map(|r| r.profit_ped).collect::<Vec<_>>());
                    let avg_ret_normal  = stat_mean(&normal_runs.iter().map(|r| r.return_pct_tt).collect::<Vec<_>>());
                    let avg_prof_normal = stat_mean(&normal_runs.iter().map(|r| r.profit_ped).collect::<Vec<_>>());

                    view! {
                        <div class="space-y-3 text-sm ">
                            <div class="grid grid-cols-2 gap-3 ">
                                <div class="bg-purple-50 border border-purple-200 rounded p-3 ">
                                    <div class="font-semibold text-purple-800 mb-1 ">"🎉 Event-Runs"</div>
                                    <div>"Anzahl: " <strong>{event_runs.len()}</strong></div>
                                    <div>"Ø Return TT: " <strong>{format!("{:.1}%", avg_ret_event)}</strong></div>
                                    <div>"Ø Profit: " <strong>{format!("{:+.2} PED", avg_prof_event)}</strong></div>
                                </div>
                                <div class="bg-gray-50 border rounded p-3 ">
                                    <div class="font-semibold text-gray-700 mb-1 ">"📋 Normal-Runs"</div>
                                    <div>"Anzahl: " <strong>{normal_runs.len()}</strong></div>
                                    <div>"Ø Return TT: " <strong>{format!("{:.1}%", avg_ret_normal)}</strong></div>
                                    <div>"Ø Profit: " <strong>{format!("{:+.2} PED", avg_prof_normal)}</strong></div>
                                </div>
                            </div>
                            {if !normal_runs.is_empty() {
                                let diff = avg_ret_event - avg_ret_normal;
                                let diff_cls = if diff >= 0.0 { "text-green-600 font-bold" } else { "text-red-500 font-bold" };
                                view!{
                                    <div class="text-sm ">
                                        "Event vs Normal: "
                                        <span class=diff_cls>{format!("{:+.1}% Return", diff)}</span>
                                    </div>
                                }.into_any()
                            } else {
                                view!{<div></div>}.into_any()
                            }}
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 1: Return-Trend-Regression ───────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"📈 Return-Trend"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.len() < 3 {
                        return view!{<p class="text-gray-400 text-sm italic ">"Mindestens 3 Runs nötig."</p>}.into_any();
                    }
                    let filtered_returns: Vec<f64> = filtered.iter().map(|r| r.return_pct_tt).collect();
                    let n = filtered_returns.len() as f64;
                    let sum_x: f64 = (0..filtered_returns.len()).map(|i| i as f64).sum();
                    let sum_y: f64 = filtered_returns.iter().sum();
                    let sum_xy: f64 = filtered_returns.iter().enumerate().map(|(i, y)| i as f64 * y).sum();
                    let sum_xx: f64 = (0..filtered_returns.len()).map(|i| (i as f64).powi(2)).sum();
                    let denom = n * sum_xx - sum_x * sum_x;
                    let slope = if denom.abs() > 1e-10 { (n * sum_xy - sum_x * sum_y) / denom } else { 0.0 };
                    let mean_ret = sum_y / n;
                    let (trend_icon, trend_cls) = if slope > 0.05 {
                        ("📈", "text-green-600 font-bold")
                    } else if slope < -0.05 {
                        ("📉", "text-red-500 font-bold")
                    } else {
                        ("➡️", "text-gray-600")
                    };
                    // Last 10 runs for mini table
                    let last10: Vec<&SavedRun> = filtered.iter().rev().take(10).rev().copied().collect();
                    view! {
                        <div class="space-y-2 ">
                            <div class="text-sm ">
                                "Trend: "
                                <span class=trend_cls>{format!("{}{:.3}%/Run {}", if slope >= 0.0 {"+"} else {""}, slope, trend_icon)}</span>
                                <span class="text-xs text-gray-400 ml-2 ">{format!("(Ø {:.1}%)", mean_ret)}</span>
                            </div>
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Datum"</th>
                                        <th class="text-right p-1.5 ">"Return TT"</th>
                                        <th class="text-right p-1.5 ">"Abw. von Ø"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {last10.into_iter().map(|r| {
                                        let dev = r.return_pct_tt - mean_ret;
                                        let dev_cls = if dev >= 0.0 { "text-green-600" } else { "text-red-500" };
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-1.5 ">{format_ts(r.started_at)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", r.return_pct_tt)}</td>
                                                <td class=format!("text-right p-1.5 {}", dev_cls)>{format!("{:+.1}%", dev)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 2: Session-Längen-Optimierung ────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"⏱ Session-Längen-Optimierung"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    #[derive(Default)]
                    struct Bucket { count: usize, sum_return: f64, sum_profit: f64 }
                    let buckets_def: &[(u64, u64, &str)] = &[
                        (0, 25, "<25 Kills"),
                        (25, 50, "25–50 Kills"),
                        (50, 100, "50–100 Kills"),
                        (100, 200, "100–200 Kills"),
                        (200, u64::MAX, "200+ Kills"),
                    ];
                    let mut buckets: Vec<(&str, Bucket)> = buckets_def.iter()
                        .map(|(_, _, label)| (*label, Bucket::default()))
                        .collect();
                    for r in &filtered {
                        let k = r.stats.kills;
                        for (i, (lo, hi, _)) in buckets_def.iter().enumerate() {
                            if k >= *lo && k < *hi {
                                buckets[i].1.count += 1;
                                buckets[i].1.sum_return += r.return_pct_tt;
                                buckets[i].1.sum_profit += r.profit_ped;
                                break;
                            }
                        }
                    }
                    let best_return = buckets.iter()
                        .filter(|(_, b)| b.count > 0)
                        .map(|(_, b)| b.sum_return / b.count as f64)
                        .fold(f64::NEG_INFINITY, f64::max);
                    view! {
                        <table class="w-full text-xs border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-1.5 ">"Kill-Anzahl"</th>
                                    <th class="text-right p-1.5 ">"Runs"</th>
                                    <th class="text-right p-1.5 ">"Ø Return TT"</th>
                                    <th class="text-right p-1.5 ">"Ø Profit"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {buckets.into_iter().filter(|(_, b)| b.count > 0).map(|(label, b)| {
                                    let avg_ret = b.sum_return / b.count as f64;
                                    let avg_prof = b.sum_profit / b.count as f64;
                                    let is_best = (avg_ret - best_return).abs() < 0.001;
                                    let row_cls = if is_best { "border-b bg-green-50 font-semibold text-green-800" } else { "border-b hover:bg-gray-50" };
                                    view! {
                                        <tr class=row_cls>
                                            <td class="p-1.5 ">{label} {if is_best {" ✅"} else {""}}</td>
                                            <td class="text-right p-1.5 ">{b.count}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.1}%", avg_ret)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:+.2} PED", avg_prof)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </section>

            // ── Feature 4: Wochentags-Analyse ────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"📅 Wochentags-Analyse"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let weekday_names = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
                    let mut wd_count  = [0usize; 7];
                    let mut wd_return = [0f64; 7];
                    let mut wd_profit = [0f64; 7];
                    for r in &filtered {
                        let wd = ((r.started_at / 86400 + 4) % 7) as usize;
                        wd_count[wd]  += 1;
                        wd_return[wd] += r.return_pct_tt;
                        wd_profit[wd] += r.profit_ped;
                    }
                    view! {
                        <table class="w-full text-xs border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-1.5 ">"Wochentag"</th>
                                    <th class="text-right p-1.5 ">"Runs"</th>
                                    <th class="text-right p-1.5 ">"Ø Return"</th>
                                    <th class="text-right p-1.5 ">"Ø Profit"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {(0..7usize).filter(|&i| wd_count[i] > 0).map(|i| {
                                    let avg_ret  = wd_return[i] / wd_count[i] as f64;
                                    let avg_prof = wd_profit[i] / wd_count[i] as f64;
                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-1.5 ">{weekday_names[i]}</td>
                                            <td class="text-right p-1.5 ">{wd_count[i]}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.1}%", avg_ret)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:+.2} PED", avg_prof)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </section>

            // ── Feature 5: Damage-per-PED Analyse ────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"⚔️ Schaden-pro-PED Analyse"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .filter(|r| r.total_cost_ped > 0.0 && r.stats.total_damage > 0.0)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let dpps: Vec<f64> = filtered.iter()
                        .map(|r| r.stats.total_damage / r.total_cost_ped)
                        .collect();
                    let avg_dpp = stat_mean(&dpps);
                    // Trend: compare last third vs first third
                    let third = (dpps.len() / 3).max(1);
                    let early_avg = stat_mean(&dpps[..third]);
                    let late_avg  = stat_mean(&dpps[dpps.len().saturating_sub(third)..]);
                    let trend_diff = late_avg - early_avg;
                    let (trend_icon, trend_cls) = if trend_diff > 0.1 {
                        ("📈", "text-green-600")
                    } else if trend_diff < -0.1 {
                        ("📉", "text-red-500")
                    } else {
                        ("➡️", "text-gray-600")
                    };
                    view! {
                        <div class="text-sm space-y-1 ">
                            <div>"Ø Schaden pro PED: " <strong>{format!("{:.2} pts/PED", avg_dpp)}</strong></div>
                            <div>"Trend: " <span class=trend_cls>{format!("{} ({:+.2} pts/PED)", trend_icon, trend_diff)}</span></div>
                            <p class="text-xs text-gray-400 ">"Höherer Wert = effizienteres Loadout (mehr Schaden pro ausgegebenem PED)"</p>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 6: Heilungs-Kosten-Analyse ───────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🩺 Heilungs-Kosten-Analyse"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .filter(|r| r.total_cost_ped > 0.0)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let fap_pcts: Vec<f64> = filtered.iter()
                        .map(|r| r.fap_decay_ped / r.total_cost_ped * 100.0)
                        .collect();
                    let avg_fap_pct = stat_mean(&fap_pcts);
                    let high_fap = avg_fap_pct > 15.0;
                    view! {
                        <div class="text-sm space-y-2 ">
                            <div>"Ø FAP-Kosten-Anteil: " <strong>{format!("{:.1}%", avg_fap_pct)}</strong></div>
                            {if high_fap {
                                view!{
                                    <div class="bg-orange-50 border border-orange-300 text-orange-800 rounded px-3 py-2 text-sm ">
                                        "⚠️ FAP-Kosten hoch — günstigere Rüstung kann Schaden reduzieren."
                                    </div>
                                }.into_any()
                            } else {
                                view!{<div></div>}.into_any()
                            }}
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Datum"</th>
                                        <th class="text-right p-1.5 ">"FAP-Decay"</th>
                                        <th class="text-right p-1.5 ">"Gesamtkosten"</th>
                                        <th class="text-right p-1.5 ">"FAP-Anteil"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {filtered.iter().rev().take(10).map(|r| {
                                        let fap_pct = r.fap_decay_ped / r.total_cost_ped * 100.0;
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-1.5 ">{format_ts(r.started_at)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.4} PED", r.fap_decay_ped)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.4} PED", r.total_cost_ped)}</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", fap_pct)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Skill-Progress-Prognose (logarithmisch) ───────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🎓 Skill-Progress-Prognose (logarithmisch)"</h3>
                <p class="text-xs text-gray-500 ">
                    "EU-Skill-Gains sind logarithmisch: je höher das Level, desto weniger Punkte pro Aktion. "
                    "Modell: dS/dt = k/S → T = (N/r)·(1 + N/2S). "
                    "Referenz-Level in Einstellungen eintragen für maximale Genauigkeit."
                </p>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }

                    // Skill-Referenz-Level parsen: "Evader=450,Longblades=320"
                    let skill_refs: HashMap<String, f64> = skill_refs_str.get()
                        .split(',')
                        .filter_map(|s| {
                            let mut p = s.trim().splitn(2, '=');
                            let name = p.next()?.trim().to_string();
                            let pts  = p.next()?.trim().parse::<f64>().ok()?;
                            if name.is_empty() { return None; }
                            Some((name, pts))
                        })
                        .collect();

                    let total_secs: u64 = filtered.iter().map(|r| r.stopped_at.saturating_sub(r.started_at)).sum();
                    let total_cost: f64 = filtered.iter().map(|r| r.total_cost_ped).sum();
                    let hours = total_secs as f64 / 3600.0;
                    if hours < 0.01 {
                        return view!{<p class="text-gray-400 text-sm italic ">"Zu wenig Daten."</p>}.into_any();
                    }
                    let cost_per_hour = if hours > 0.0 { total_cost / hours } else { 0.0 };

                    // Gains aufteilen: frühe Runs (erstes Drittel) vs. späte Runs (letztes Drittel)
                    let n_runs = filtered.len();
                    let third  = (n_runs / 3).max(1);
                    let early_secs: f64 = filtered[..third].iter()
                        .map(|r| r.stopped_at.saturating_sub(r.started_at) as f64).sum();
                    let late_secs: f64  = filtered[n_runs.saturating_sub(third)..].iter()
                        .map(|r| r.stopped_at.saturating_sub(r.started_at) as f64).sum();

                    // Per-Skill aggregieren
                    let mut skill_totals: HashMap<String, f64> = HashMap::new();
                    let mut skill_early:  HashMap<String, f64> = HashMap::new();
                    let mut skill_late:   HashMap<String, f64> = HashMap::new();

                    for (i, r) in filtered.iter().enumerate() {
                        for (skill, &amount) in &r.stats.skill_gains {
                            *skill_totals.entry(skill.clone()).or_insert(0.0) += amount;
                            if i < third {
                                *skill_early.entry(skill.clone()).or_insert(0.0) += amount;
                            }
                            if i >= n_runs.saturating_sub(third) {
                                *skill_late.entry(skill.clone()).or_insert(0.0) += amount;
                            }
                        }
                    }

                    if skill_totals.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Skill-Gains erfasst."</p>}.into_any();
                    }

                    let mut skills: Vec<(String, f64)> = skill_totals.into_iter().collect();
                    skills.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                    const TARGET_N: f64 = 100.0;

                    view! {
                        <div class="space-y-3 ">
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Skill"</th>
                                        <th class="text-right p-1.5 ">"Rate (pkt/h)"</th>
                                        <th class="text-right p-1.5 ">"Rate-Trend"</th>
                                        <th class="text-right p-1.5 ">"Linear (100 Pkt)"</th>
                                        <th class="text-right p-1.5 ">"Log-korrigiert"</th>
                                        <th class="text-right p-1.5 ">"PED (log)"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                {skills.into_iter().take(10).map(|(skill, total_gained)| {
                                    let r_avg  = if hours > 0.0 { total_gained / hours } else { 0.0 };

                                    // Rate-Trend: früh vs. spät
                                    let r_early = if early_secs > 0.0 {
                                        skill_early.get(&skill).copied().unwrap_or(0.0) / (early_secs / 3600.0)
                                    } else { r_avg };
                                    let r_late  = if late_secs > 0.0 {
                                        skill_late.get(&skill).copied().unwrap_or(0.0) / (late_secs / 3600.0)
                                    } else { r_avg };

                                    let trend_pct = if r_early > 0.0 {
                                        (r_late / r_early - 1.0) * 100.0
                                    } else { 0.0 };
                                    let trend_str = if n_runs >= 3 {
                                        format!("{:+.0}%", trend_pct)
                                    } else { "–".into() };
                                    let trend_cls = if trend_pct < -5.0 { "text-orange-500" }
                                                    else if trend_pct > 5.0 { "text-green-600" }
                                                    else { "text-gray-500" };

                                    // S = Referenz-Level (aus Settings) ODER aufgezeichnete Gains als Untergrenze
                                    let s_ref  = skill_refs.get(&skill).copied();
                                    let s_rec  = total_gained; // aufgezeichnete Gains seit App-Start
                                    let (s, s_label) = match s_ref {
                                        Some(ref_pts) => (ref_pts + s_rec, "Ref+Aufgez."),
                                        None          => (s_rec.max(1.0),  "nur Aufgez."),
                                    };

                                    // Aktuelle Rate = spätere Rate (repräsentiert den heutigen Stand besser)
                                    let r_current = if n_runs >= 3 && r_late > 0.0 { r_late } else { r_avg };

                                    // Lineare Prognose
                                    let t_linear = if r_current > 0.0 { TARGET_N / r_current } else { f64::INFINITY };

                                    // Logarithmische Korrektur: T_log = (N/r) * (1 + N/(2S))
                                    let correction = 1.0 + TARGET_N / (2.0 * s);
                                    let t_log = t_linear * correction;
                                    let ped_log = if t_log.is_finite() { t_log * cost_per_hour } else { f64::INFINITY };

                                    let fmt_h = |h: f64| -> String {
                                        if h.is_infinite() || h > 9999.0 { "∞".into() }
                                        else if h >= 100.0 { format!("{:.0}h", h) }
                                        else { format!("{:.1}h", h) }
                                    };

                                    view! {
                                        <tr class="border-b hover:bg-gray-50 ">
                                            <td class="p-1.5 font-medium ">{skill}</td>
                                            <td class="text-right p-1.5 font-mono ">{format!("{:.4}", r_current)}</td>
                                            <td class=format!("text-right p-1.5 font-mono {}", trend_cls)>{trend_str}</td>
                                            <td class="text-right p-1.5 text-gray-400 line-through ">{fmt_h(t_linear)}</td>
                                            <td class="text-right p-1.5 font-semibold ">{fmt_h(t_log)}</td>
                                            <td class="text-right p-1.5 ">{
                                                if ped_log.is_finite() { format!("{:.1} PED", ped_log) } else { "∞".into() }
                                            }</td>
                                        </tr>
                                        // Zeile 2: Kontext-Info
                                        <tr class="border-b ">
                                            <td colspan="6" class="px-1.5 pb-1.5 text-xs text-gray-400 ">
                                                {format!("  S={:.0} Pkt ({}) · Korrektur ×{:.2}",
                                                    s, s_label, correction)}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                                </tbody>
                            </table>
                            {if skill_refs.is_empty() {
                                Some(view!{
                                    <div class="text-xs bg-yellow-50 border border-yellow-200 rounded p-2 ">
                                        "⚠️ Kein Referenz-Level eingetragen → Prognose basiert nur auf aufgezeichneten Gains "
                                        "(unterschätzt die Korrektur). "
                                        "Einstellungen → Skill-Level-Referenz → z.B. \"Evader=450,Longblades=320\" eintragen."
                                    </div>
                                })
                            } else { None }}
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 11: Budget-Planer ─────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🗓 Budget-Planer"</h3>
                {move || {
                    let sc = sel_creature.get();
                    let all_runs = runs.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine historischen Daten."</p>}.into_any();
                    }
                    let total_kills: u64 = filtered.iter().map(|r| r.stats.kills).sum();
                    if total_kills == 0 { return view!{<p class="text-gray-400 text-sm italic ">"Keine Kill-Daten."</p>}.into_any(); }
                    let avg_cost_per_kill = filtered.iter().map(|r| r.total_cost_ped).sum::<f64>() / total_kills as f64;
                    let avg_loot_per_kill = filtered.iter().map(|r| r.stats.total_loot_value_ped).sum::<f64>() / total_kills as f64;
                    let avg_armor_per_kill = filtered.iter().map(|r| r.armor_decay_ped).sum::<f64>() / total_kills as f64;
                    let avg_return_tt = stat_mean(&filtered.iter().map(|r| r.return_pct_tt).collect::<Vec<_>>());
                    let n = budget_kills.get();
                    let ammo_total  = avg_cost_per_kill * n as f64;
                    let armor_total = avg_armor_per_kill * n as f64;
                    let loot_total  = avg_loot_per_kill * n as f64;
                    view! {
                        <div class="space-y-3 ">
                            <div class="flex items-center gap-2 text-sm ">
                                "Ziel-Kill-Anzahl: "
                                <input
                                    class="input w-24 "
                                    type="number" min="1" step="10"
                                    prop:value=move || budget_kills.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                            budget_kills.set(v);
                                        }
                                    }
                                />
                            </div>
                            <div class="bg-blue-50 border border-blue-100 rounded p-3 text-sm space-y-1 ">
                                <div class="font-medium text-blue-800 ">{format!("Für {} Kills:", n)}</div>
                                <div class="grid grid-cols-2 gap-x-4 text-xs ">
                                    <span class="text-gray-600 ">"Ammo-Budget:"</span>
                                    <span class="font-mono ">{format!("{:.2} PED", ammo_total)}</span>
                                    <span class="text-gray-600 ">"Rüstungsbudget:"</span>
                                    <span class="font-mono ">{format!("{:.2} PED", armor_total)}</span>
                                    <span class="text-gray-600 ">"Gesamt:"</span>
                                    <span class="font-mono font-semibold ">{format!("{:.2} PED", ammo_total + armor_total)}</span>
                                    <span class="text-gray-600 ">"Erwarteter Loot (TT):"</span>
                                    <span class="font-mono text-green-700 ">{format!("{:.2} PED (Ø {:.1}% TT)", loot_total, avg_return_tt)}</span>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 12: MU-Stale-Warnung (in MU-Impact-Analyse) ──────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"⏰ MU-Aktualitäts-Prüfung"</h3>
                {move || {
                    let items = mu_items.get();
                    if items.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine MU-Konfiguration."</p>}.into_any();
                    }
                    let now_sec_val = (js_sys::Date::now() / 1000.0) as u64;
                    let seven_days_sec = 7 * 86400;
                    let stale: Vec<&LootItemConfig> = items.iter()
                        .filter(|c| {
                            match c.last_updated_sec {
                                None => true,
                                Some(t) => now_sec_val.saturating_sub(t) > seven_days_sec,
                            }
                        })
                        .collect();
                    if stale.is_empty() {
                        return view!{<p class="text-green-600 text-sm ">"✅ Alle MU-Werte aktuell (< 7 Tage alt)."</p>}.into_any();
                    }
                    view! {
                        <div class="space-y-2 ">
                            <p class="text-sm text-orange-700 ">{format!("⚠️ {} Item(s) mit veralteten MU-Daten (>7 Tage):", stale.len())}</p>
                            <table class="w-full text-xs border-collapse ">
                                <thead class="border-b bg-gray-50 ">
                                    <tr>
                                        <th class="text-left p-1.5 ">"Item"</th>
                                        <th class="text-right p-1.5 ">"MU%"</th>
                                        <th class="text-right p-1.5 ">"Letztes Update"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {stale.iter().map(|c| {
                                        let age_str = match c.last_updated_sec {
                                            None => "Nie".to_string(),
                                            Some(t) => {
                                                let age_days = now_sec_val.saturating_sub(t) / 86400;
                                                format!("vor {}d", age_days)
                                            }
                                        };
                                        view! {
                                            <tr class="border-b hover:bg-gray-50 ">
                                                <td class="p-1.5 ">{c.name.clone()} " ⚠️"</td>
                                                <td class="text-right p-1.5 ">{format!("{:.1}%", c.mu_pct)}</td>
                                                <td class="text-right p-1.5 text-orange-600 ">{age_str}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Feature 13: Loadout-Effizienz aus echten Runs ─────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🔧 Loadout-Effizienz (real)"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();
                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten."</p>}.into_any();
                    }
                    let all_loadouts = loadouts.get();
                    use std::collections::BTreeMap;
                    struct LoadoutStats { count: usize, sum_return: f64, sum_profit: f64, sum_kills: u64 }
                    let mut by_loadout: BTreeMap<String, LoadoutStats> = BTreeMap::new();
                    for r in &filtered {
                        let lid = r.config.loadout_id.clone();
                        let e = by_loadout.entry(lid).or_insert(LoadoutStats { count: 0, sum_return: 0.0, sum_profit: 0.0, sum_kills: 0 });
                        e.count      += 1;
                        e.sum_return += r.return_pct_tt;
                        e.sum_profit += r.profit_ped;
                        e.sum_kills  += r.stats.kills;
                    }
                    let mut rows: Vec<(String, f64, f64, f64)> = by_loadout.into_iter().map(|(lid, st)| {
                        let name = all_loadouts.iter().find(|l| l.id == lid)
                            .map(|l| l.name.clone()).unwrap_or(lid);
                        let avg_ret  = st.sum_return / st.count as f64;
                        let avg_prof = st.sum_profit / st.count as f64;
                        let avg_kills= st.sum_kills  as f64 / st.count as f64;
                        (name, avg_ret, avg_prof, avg_kills)
                    }).collect();
                    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let best_ret = rows.first().map(|r| r.1).unwrap_or(0.0);
                    view! {
                        <table class="w-full text-xs border-collapse ">
                            <thead class="border-b bg-gray-50 ">
                                <tr>
                                    <th class="text-left p-1.5 ">"Rang"</th>
                                    <th class="text-left p-1.5 ">"Loadout"</th>
                                    <th class="text-right p-1.5 ">"Ø Return TT"</th>
                                    <th class="text-right p-1.5 ">"Ø Profit"</th>
                                    <th class="text-right p-1.5 ">"Ø Kills"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows.into_iter().enumerate().map(|(i, (name, avg_ret, avg_prof, avg_kills))| {
                                    let is_best = (avg_ret - best_ret).abs() < 0.001;
                                    let row_cls = if is_best { "border-b bg-green-50 font-semibold text-green-800" } else { "border-b hover:bg-gray-50" };
                                    view! {
                                        <tr class=row_cls>
                                            <td class="p-1.5 ">{i + 1}</td>
                                            <td class="p-1.5 ">{name} {if is_best {" ✅"} else {""}}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.1}%", avg_ret)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:+.2} PED", avg_prof)}</td>
                                            <td class="text-right p-1.5 ">{format!("{:.0}", avg_kills)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </section>

            // ── Statistik & Session-Optimierung ──────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"📐 Statistik & Session-Optimierung"</h3>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();

                    if filtered.is_empty() {
                        return view!{<p class="text-gray-400 text-sm italic ">"Keine Daten – Kreatur wählen oder Runs erfassen."</p>}.into_any();
                    }

                    // Kill-Loot-Werte aggregieren
                    let mut kill_values: Vec<f64> = filtered.iter()
                        .flat_map(|r| r.stats.kill_loots.iter().map(|k| k.value_ped))
                        .collect();
                    kill_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                    let n_kills = kill_values.len();
                    let mu     = stat_mean(&kill_values);
                    let sigma  = stat_std(&kill_values);
                    let cv     = if mu > 0.0 { sigma / mu } else { 0.0 };

                    let p5  = stat_percentile(&kill_values, 5.0);
                    let p25 = stat_percentile(&kill_values, 25.0);
                    let p50 = stat_percentile(&kill_values, 50.0);
                    let p75 = stat_percentile(&kill_values, 75.0);
                    let p95 = stat_percentile(&kill_values, 95.0);

                    // Return-CI: über Run-Werte
                    let returns: Vec<f64> = filtered.iter().map(|r| r.return_pct_tt).collect();
                    let n_runs = returns.len();
                    let ret_mean = stat_mean(&returns);
                    let ret_std  = stat_std(&returns);
                    let ret_ci   = if n_runs > 1 { 1.96 * ret_std / (n_runs as f64).sqrt() } else { 0.0 };

                    // Mindest-Kills für ±10% Genauigkeit
                    let min_kills = min_kills_for_significance(sigma, mu, 10.0);
                    // Kosten-Proxy: avg Ammo-Kosten pro Kill aus Runs
                    let avg_cost_per_kill: f64 = {
                        let tot_cost:  f64 = filtered.iter().map(|r| r.ammo_cost_ped).sum();
                        let tot_kills: u64 = filtered.iter().map(|r| r.stats.kills).sum();
                        if tot_kills > 0 { tot_cost / tot_kills as f64 } else { 0.0 }
                    };
                    let min_budget = min_kills as f64 * avg_cost_per_kill;

                    // Kelly-Kriterium
                    let kelly = kelly_fraction(&filtered);

                    // Risiko-Ampel
                    let (risk_icon, risk_label, risk_cls) = if cv < 0.8 && n_runs >= 30 {
                        ("🟢", "Niedrig", "text-green-600")
                    } else if cv > 1.5 || n_runs < 10 {
                        ("🔴", "Hoch", "text-red-500")
                    } else {
                        ("🟡", "Mittel", "text-yellow-600")
                    };

                    let vol_cls = if cv < 1.0 { "text-green-600" } else if cv < 2.0 { "text-yellow-600" } else { "text-red-500" };
                    let vol_label = if cv < 1.0 { "Niedrig" } else if cv < 2.0 { "Mittel" } else { "Hoch" };

                    view! {
                        <div class="space-y-4 ">
                            // Block 1: Kill-Loot-Verteilung
                            <div class="bg-white border rounded p-3 space-y-2 ">
                                <div class="font-medium text-sm ">"Kill-Loot-Verteilung"</div>
                                <div class="text-xs text-gray-500 flex flex-wrap gap-x-4 gap-y-1 ">
                                    <span>{n_kills} " Kills"</span>
                                    <span>"μ = " {format!("{:.4} PED", mu)}</span>
                                    <span>"σ = " {format!("{:.4}", sigma)}</span>
                                    <span>"CV = " <span class=vol_cls>{format!("{:.2}", cv)} " (" {vol_label} ")"</span></span>
                                </div>
                                <div class="grid grid-cols-5 gap-1 text-xs text-center ">
                                    {[("P5", p5), ("P25", p25), ("P50\n(Med.)", p50), ("P75", p75), ("P95", p95)].iter().map(|(label, val)| {
                                        view!{
                                            <div class="bg-gray-50 border rounded p-1 ">
                                                <div class="font-semibold text-gray-500 ">{*label}</div>
                                                <div class="font-mono ">{format!("{:.3}", val)}</div>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>

                            // Block 2: Konfidenzintervall Return
                            <div class="bg-white border rounded p-3 space-y-1 text-sm ">
                                <div class="font-medium ">"Return-Quote (95%-Konfidenzintervall)"</div>
                                {if n_runs < 5 {
                                    view!{
                                        <p class="text-xs text-yellow-600 italic ">"⚠ Zu wenig Runs (" {n_runs} ") für belastbare Aussage – mindestens 5 empfohlen."</p>
                                    }.into_any()
                                } else {
                                    view!{
                                        <div class="space-y-1 ">
                                            <div class="flex gap-4 text-xs ">
                                                <span>"Ø Return: " <strong>{format!("{:.1}%", ret_mean)}</strong></span>
                                                <span>"σ Runs: " <strong>{format!("{:.1}%", ret_std)}</strong></span>
                                                <span>"n Runs: " <strong>{n_runs}</strong></span>
                                            </div>
                                            <div class="text-xs font-mono bg-blue-50 border border-blue-100 rounded px-2 py-1 ">
                                                "95%-CI: [" {format!("{:.1}%", ret_mean - ret_ci)} " – " {format!("{:.1}%", ret_mean + ret_ci)} "]"
                                            </div>
                                        </div>
                                    }.into_any()
                                }}
                            </div>

                            // Block 3: Mindest-Session
                            <div class="bg-white border rounded p-3 space-y-1 text-sm ">
                                <div class="font-medium ">"Mindest-Session für ±10% Genauigkeit (95% Konfidenz)"</div>
                                {if n_kills < 10 {
                                    view!{<p class="text-xs text-yellow-600 italic ">"Zu wenig Kill-Daten für Berechnung."</p>}.into_any()
                                } else {
                                    view!{
                                        <div class="flex flex-wrap gap-x-6 gap-y-1 text-xs ">
                                            <span>"Mindest-Kills: " <strong>{min_kills}</strong></span>
                                            {if avg_cost_per_kill > 0.0 { Some(view!{
                                                <span>"Mindest-Budget: " <strong>{format!("{:.2} PED", min_budget)}</strong></span>
                                            })} else { None }}
                                        </div>
                                    }.into_any()
                                }}
                                <p class="text-xs text-gray-400 ">"Formel: n = (1.96 · σ / 0.1·μ)²  |  Varianz der Kill-Loots als Grundlage."</p>
                            </div>

                            // Block 4: Kelly-Kriterium
                            <div class="bg-white border rounded p-3 space-y-1 text-sm ">
                                <div class="font-medium ">"Kelly-Kriterium (optimale Sessiongröße)"</div>
                                {if n_runs < 3 {
                                    view!{<p class="text-xs text-yellow-600 italic ">"Mindestens 3 Runs nötig."</p>}.into_any()
                                } else {
                                    let win_runs: usize = filtered.iter().filter(|r| r.profit_ped > 0.0).count();
                                    view!{
                                        <div class="space-y-1 ">
                                            <div class="flex flex-wrap gap-x-6 text-xs ">
                                                <span>"Gewinn-Runs: " <strong>{format!("{}/{} ({:.0}%)", win_runs, n_runs, win_runs as f64 / n_runs as f64 * 100.0)}</strong></span>
                                                <span>"Kelly-Anteil: " <strong>{format!("{:.1}%", kelly * 100.0)}</strong></span>
                                            </div>
                                            <div class="text-xs font-mono bg-green-50 border border-green-100 rounded px-2 py-1 ">
                                                "Bei 1000 PED Kapital: " {format!("{:.0} PED", kelly * 1000.0)} " pro Session empfohlen"
                                            </div>
                                        </div>
                                    }.into_any()
                                }}
                                <p class="text-xs text-gray-400 ">"Basiert auf Gewinn/Verlust-Verhältnis der bisherigen Runs. Max. 25% Bankroll."</p>
                            </div>

                            // Block 5: Risiko-Ampel + Erklärung
                            <div class="bg-white border rounded p-3 space-y-2 ">
                                <div class="flex items-center gap-2 ">
                                    <span class="text-xl ">{risk_icon}</span>
                                    <span class="font-medium ">"Risiko-Einschätzung: " <span class=risk_cls>{risk_label}</span></span>
                                </div>
                                <p class="text-xs text-gray-500 leading-relaxed ">
                                    "Das Gesetz der großen Zahlen hilft, die Varianz zu reduzieren — nicht die "
                                    "erwartete Rückgabequote zu verbessern. Mit mehr Kills nähert sich dein "
                                    "Return dem wahren Erwartungswert. Das schützt aber nicht vor negativem EV: "
                                    "Wenn EU für diese Kreatur weniger als 100% zurückgibt, "
                                    "bleibt das Erwartungswert-Defizit bestehen."
                                </p>
                            </div>
                        </div>
                    }.into_any()
                }}
            </section>

            // ── Wave-Loot-Theorien ─────────────────────────────────────────────
            <section class="space-y-3 ">
                <h3 class="font-medium ">"🌊 Wave-Loot-Theorien"</h3>
                <p class="text-xs text-gray-500 ">
                    "Statistische Tests auf Basis deiner Kill-Loot-Zeitreihe. "
                    "Konfidenz steigt automatisch mit mehr Kills. Mindestens 30 Kills benötigt."
                </p>
                {move || {
                    let all_runs = runs.get();
                    let sc = sel_creature.get();
                    let filtered: Vec<&SavedRun> = all_runs.iter()
                        .filter(|r| sc.is_empty() || r.config.creature == sc)
                        .collect();

                    // Alle Kill-Loots in chronologischer Reihenfolge
                    let all_loots: Vec<f64> = filtered.iter()
                        .flat_map(|r| r.stats.kill_loots.iter().map(|k| k.value_ped))
                        .collect();

                    if all_loots.len() < 30 {
                        return view!{
                            <p class="text-gray-400 text-sm italic ">
                                {format!("Mindestens 30 Kill-Loots benötigt ({} vorhanden).", all_loots.len())}
                            </p>
                        }.into_any();
                    }

                    let n = all_loots.len();
                    let mean = stat_mean(&all_loots);
                    let std  = stat_std(&all_loots);

                    // ── T1: Loot-Pool-Depletion ────────────────────────────────
                    // Hypothese: Nach Big-Loot (≥5× Kosten/Kill) sind die nächsten 10 Kills unterdurchschnittlich.
                    let t1 = {
                        let window = 10usize;
                        let big_threshold = (cost_per_kill_ref.get() * 5.0).max(0.01);
                        let mut post_samples: Vec<f64> = vec![];
                        for i in 0..n {
                            if all_loots[i] >= big_threshold && i + window < n {
                                let post_mean = stat_mean(&all_loots[i+1..i+1+window]);
                                post_samples.push(post_mean);
                            }
                        }
                        let events = post_samples.len();
                        if events < 3 {
                            WaveResult { events, obs: 0.0, baseline: mean, t_or_z: 0.0, confirmed: false, tendency: false, label: "zu wenig Big-Loots".into() }
                        } else {
                            let post_mean = stat_mean(&post_samples);
                            let post_std  = stat_std(&post_samples);
                            let t = if post_std > 0.0 { (post_mean - mean) / (post_std / (events as f64).sqrt()) } else { 0.0 };
                            // Einseitig: erwarten t < -1.65 (p < 0.05)
                            let confirmed = t < -1.65;
                            let tendency  = t < -0.84;   // p < 0.20
                            WaveResult { events, obs: post_mean, baseline: mean, t_or_z: t, confirmed, tendency,
                                label: format!("Ø Post-BigLoot (nächste {window} Kills): {post_mean:.3} PED vs Ø {mean:.3} PED") }
                        }
                    };

                    // ── T2: Dry-Streak-Bounce ─────────────────────────────────
                    // Hypothese: Nach ≥5 Sub-Ø-Kills in Folge ist der nächste Loot überdurchschnittlich.
                    let t2 = {
                        let dry_len = 5usize;
                        let mut bounce_above = 0u32;
                        let mut bounce_total = 0u32;
                        for i in dry_len..n {
                            let is_dry_streak = (0..dry_len).all(|j| all_loots[i-dry_len+j] < mean);
                            if is_dry_streak {
                                bounce_total += 1;
                                if all_loots[i] >= mean { bounce_above += 1; }
                            }
                        }
                        let p_base = all_loots.iter().filter(|&&v| v >= mean).count() as f64 / n as f64;
                        if bounce_total < 5 {
                            WaveResult { events: bounce_total as usize, obs: 0.0, baseline: p_base, t_or_z: 0.0, confirmed: false, tendency: false,
                                label: "zu wenig Dry-Streaks ≥5".into() }
                        } else {
                            let p_bounce = bounce_above as f64 / bounce_total as f64;
                            let z = (p_bounce - p_base) / (p_base * (1.0 - p_base) / bounce_total as f64).sqrt();
                            let confirmed = z > 1.65;
                            let tendency  = z > 0.84;
                            WaveResult { events: bounce_total as usize, obs: p_bounce, baseline: p_base, t_or_z: z, confirmed, tendency,
                                label: format!("P(≥Ø nach 5 Dry-Kills): {:.0}% vs Basis {:.0}%", p_bounce*100.0, p_base*100.0) }
                        }
                    };

                    // ── T3: Hot-Streak-Momentum ───────────────────────────────
                    // Hypothese: Nach 3 überdurchschnittlichen Kills in Folge bleibt der nächste auch überdurchschnittlich.
                    let t3 = {
                        let hot_len = 3usize;
                        let mut hot_above = 0u32;
                        let mut hot_total = 0u32;
                        for i in hot_len..n {
                            let is_hot = (0..hot_len).all(|j| all_loots[i-hot_len+j] >= mean);
                            if is_hot {
                                hot_total += 1;
                                if all_loots[i] >= mean { hot_above += 1; }
                            }
                        }
                        let p_base = all_loots.iter().filter(|&&v| v >= mean).count() as f64 / n as f64;
                        if hot_total < 5 {
                            WaveResult { events: hot_total as usize, obs: 0.0, baseline: p_base, t_or_z: 0.0, confirmed: false, tendency: false,
                                label: "zu wenig Hot-Streaks ≥3".into() }
                        } else {
                            let p_hot = hot_above as f64 / hot_total as f64;
                            let z = (p_hot - p_base) / (p_base * (1.0 - p_base) / hot_total as f64).sqrt();
                            let confirmed = z > 1.65;
                            let tendency  = z > 0.84;
                            WaveResult { events: hot_total as usize, obs: p_hot, baseline: p_base, t_or_z: z, confirmed, tendency,
                                label: format!("P(≥Ø nach 3 Hot-Kills): {:.0}% vs Basis {:.0}%", p_hot*100.0, p_base*100.0) }
                        }
                    };

                    // ── T4: Lag-1-Autokorrelation ─────────────────────────────
                    // r > 0: Momentum (gut folgt gut), r < 0: Mean-Reversion (gut folgt schlecht)
                    let t4 = {
                        let m1 = stat_mean(&all_loots[..n-1]);
                        let m2 = stat_mean(&all_loots[1..]);
                        let s1 = stat_std(&all_loots[..n-1]);
                        let s2 = stat_std(&all_loots[1..]);
                        let r = if s1 > 0.0 && s2 > 0.0 {
                            let cov: f64 = (0..n-1).map(|i| (all_loots[i] - m1) * (all_loots[i+1] - m2)).sum::<f64>() / (n-2) as f64;
                            cov / (s1 * s2)
                        } else { 0.0 };
                        // 95%-CI: r ± 1.96/sqrt(n)
                        let ci = 1.96 / (n as f64).sqrt();
                        let z = r / (1.0 / ((n-2) as f64).sqrt()); // approx
                        let confirmed = r.abs() > ci;
                        let tendency  = r.abs() > ci * 0.5;
                        let direction = if r > 0.0 { "Momentum (pos. Korrelation)" } else { "Mean-Reversion (neg. Korrelation)" };
                        WaveResult { events: n - 1, obs: r, baseline: 0.0, t_or_z: z, confirmed, tendency,
                            label: format!("r = {:.3} ± {:.3} → {}", r, ci, direction) }
                    };

                    // ── T5: Zyklus-Länge ──────────────────────────────────────
                    // Durchschnittlicher Abstand zwischen lokalen Loot-Peaks (> Nachbarn und > Ø)
                    let t5 = {
                        let mut peak_positions: Vec<usize> = vec![];
                        for i in 1..n-1 {
                            if all_loots[i] > all_loots[i-1] && all_loots[i] > all_loots[i+1] && all_loots[i] >= mean {
                                peak_positions.push(i);
                            }
                        }
                        let events = peak_positions.len();
                        if events < 3 {
                            WaveResult { events, obs: 0.0, baseline: 0.0, t_or_z: 0.0, confirmed: false, tendency: false,
                                label: "zu wenig Peaks".into() }
                        } else {
                            let gaps: Vec<f64> = peak_positions.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
                            let avg_gap = stat_mean(&gaps);
                            let gap_std = stat_std(&gaps);
                            let cv_gap  = if avg_gap > 0.0 { gap_std / avg_gap } else { 0.0 };
                            // Niedriger CV = regelmäßige Zyklen (< 0.5 = ziemlich regelmäßig)
                            let confirmed = cv_gap < 0.4;
                            let tendency  = cv_gap < 0.7;
                            WaveResult { events, obs: avg_gap, baseline: cv_gap, t_or_z: 0.0, confirmed, tendency,
                                label: format!("Ø Zyklus: {:.0} Kills (CV={:.2}, {} Peaks)", avg_gap, cv_gap, events) }
                        }
                    };

                    let _ = std;  // used in CI display below

                    view! {
                        <div class="space-y-3 ">
                            // Legende
                            <div class="flex gap-4 text-xs text-gray-500 ">
                                <span>"🟢 Bestätigt (p&lt;0.05)"</span>
                                <span>"🟡 Tendenz (p&lt;0.20)"</span>
                                <span>"🔴 Nicht bestätigt"</span>
                                <span class="ml-auto font-mono ">{format!("n = {} Kills, Ø = {:.3} PED", n, mean)}</span>
                            </div>

                            // T1
                            {wave_card(
                                "T1: Loot-Pool-Depletion",
                                "Nach einem Big-Loot (≥5× Kosten/Kill) sind die nächsten 10 Kills unterdurchschnittlich.",
                                &t1,
                            )}
                            // T2
                            {wave_card(
                                "T2: Dry-Streak-Bounce",
                                "Nach ≥5 Sub-Ø-Kills in Folge ist der nächste Loot überdurchschnittlich.",
                                &t2,
                            )}
                            // T3
                            {wave_card(
                                "T3: Hot-Streak-Momentum",
                                "Nach 3 überdurchschnittlichen Kills in Folge bleibt der nächste auch über Ø.",
                                &t3,
                            )}
                            // T4
                            {wave_card(
                                "T4: Autokorrelation (Lag 1)",
                                "Korreliert ein Kill-Loot mit dem nächsten? (Momentum vs. Mean-Reversion)",
                                &t4,
                            )}
                            // T5
                            {wave_card(
                                "T5: Zyklus-Länge",
                                "Regelmäßiger Abstand zwischen Loot-Peaks = Hinweis auf Wellenmechanik.",
                                &t5,
                            )}

                            <p class="text-xs text-gray-400 italic leading-relaxed ">
                                "⚠️ Statistische Evidenz ≠ Kausalität. Selbst bei p&lt;0.05 kann ein Muster Zufall sein. "
                                "Die Konfidenz steigt mit mehr Daten — mindestens 200+ Kills für belastbare Aussagen empfohlen."
                            </p>
                        </div>
                    }.into_any()
                }}
            </section>
        </div>
    }
}

// ─── Wave-Loot-Hilfstrukturen ─────────────────────────────────────────────────

#[allow(dead_code)]
struct WaveResult {
    events:    usize,
    obs:       f64,
    baseline:  f64,
    t_or_z:    f64,
    confirmed: bool,
    tendency:  bool,
    label:     String,
}

fn wave_card(title: &str, hypothesis: &str, r: &WaveResult) -> impl IntoView {
    let (icon, border_cls, badge) = if r.events < 5 {
        ("⚪", "border-gray-200 bg-gray-50", "Zu wenig Daten")
    } else if r.confirmed {
        ("🟢", "border-green-200 bg-green-50", "Bestätigt")
    } else if r.tendency {
        ("🟡", "border-yellow-200 bg-yellow-50", "Tendenz")
    } else {
        ("🔴", "border-red-100 bg-red-50", "Nicht bestätigt")
    };
    let title   = title.to_owned();
    let hypo    = hypothesis.to_owned();
    let label   = r.label.clone();
    let events  = r.events;
    let t_val   = r.t_or_z;
    let badge   = badge.to_owned();
    let t_str   = if r.events >= 5 { format!("z/t = {:.2}", t_val) } else { String::new() };

    view! {
        <div class=format!("border rounded p-3 space-y-1 {}", border_cls)>
            <div class="flex items-center justify-between ">
                <span class="font-semibold text-sm ">{icon} " " {title}</span>
                <span class="text-xs font-mono px-2 py-0.5 rounded border ">{badge}</span>
            </div>
            <p class="text-xs text-gray-600 italic ">{hypo}</p>
            <div class="text-xs font-mono text-gray-800 ">{label}</div>
            <div class="text-xs text-gray-400 ">
                "n=" {events} " Ereignisse"
                {if !t_str.is_empty() { format!(" · {}", t_str) } else { String::new() }}
            </div>
        </div>
    }
}

// ─── Statistische Hilfsfunktionen ─────────────────────────────────────────────

fn stat_mean(v: &[f64]) -> f64 {
    if v.is_empty() { return 0.0; }
    v.iter().sum::<f64>() / v.len() as f64
}

fn stat_std(v: &[f64]) -> f64 {
    if v.len() < 2 { return 0.0; }
    let m = stat_mean(v);
    let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64;
    var.sqrt()
}

fn stat_percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Mindest-Kills für 95%-Konfidenzintervall mit ±`error_pct`% Fehlertoleranz.
fn min_kills_for_significance(std: f64, mean: f64, error_pct: f64) -> u64 {
    let e = mean * error_pct / 100.0;
    if e <= 0.0 || std <= 0.0 { return 0; }
    ((1.96 * std / e).powi(2)).ceil() as u64
}

/// Timestamp → "DD.MM.YYYY HH:MM"
fn format_ts(ts: u64) -> String {
    let s = ts % 60;
    let m = (ts / 60) % 60;
    let h = (ts / 3600) % 24;
    let days = ts / 86400;
    // Very simple Gregorian estimate from Unix epoch
    let y400 = days / 146097;
    let rem  = days % 146097;
    let y100 = (rem / 36524).min(3);
    let rem  = rem - y100 * 36524;
    let y4   = rem / 1461;
    let rem  = rem % 1461;
    let y1   = (rem / 365).min(3);
    let doy  = rem - y1 * 365;
    let year = y400 * 400 + y100 * 100 + y4 * 4 + y1 + 1970;
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let months: &[u64] = if leap {
        &[31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        &[31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut rem_d = doy;
    let mut month = 1u64;
    for &ml in months {
        if rem_d < ml { break; }
        rem_d -= ml;
        month += 1;
    }
    let day = rem_d + 1;
    format!("{:02}.{:02}.{} {:02}:{:02}:{:02}", day, month, year, h, m, s)
}

/// Kelly-Kriterium: empfohlener Bankroll-Anteil (0..0.25) basierend auf Runs.
fn kelly_fraction(filtered: &[&SavedRun]) -> f64 {
    if filtered.len() < 3 { return 0.0; }
    let wins:   Vec<f64> = filtered.iter().filter(|r| r.profit_ped > 0.0).map(|r| r.profit_ped).collect();
    let losses: Vec<f64> = filtered.iter().filter(|r| r.profit_ped <= 0.0).map(|r| r.profit_ped.abs()).collect();
    if wins.is_empty() || losses.is_empty() { return 0.0; }
    let p = wins.len() as f64 / filtered.len() as f64;
    let q = 1.0 - p;
    let avg_win  = stat_mean(&wins);
    let avg_loss = stat_mean(&losses);
    if avg_loss <= 0.0 { return 0.0; }
    let b = avg_win / avg_loss;
    ((p * b - q) / b).max(0.0).min(0.25)
}
