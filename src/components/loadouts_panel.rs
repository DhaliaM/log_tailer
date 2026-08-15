use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;

use crate::domain::types::{Amplifier, ArmorProfile, FapProfile, Loadout, Weapon};
use crate::persistence::idb::Db;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubTab { Weapons, Amps, Armors, Faps, Loadouts }

#[component]
pub fn LoadoutsPanel() -> impl IntoView {
    let (db, set_db) = signal_local::<Option<Db>>(None);
    let active = RwSignal::new(SubTab::Loadouts);

    let (weapons,  set_weapons)  = signal_local::<Vec<Weapon>>(vec![]);
    let (amps,     set_amps)     = signal_local::<Vec<Amplifier>>(vec![]);
    let (armors,   set_armors)   = signal_local::<Vec<ArmorProfile>>(vec![]);
    let (faps,     set_faps)     = signal_local::<Vec<FapProfile>>(vec![]);
    let (loadouts, set_loadouts) = signal_local::<Vec<Loadout>>(vec![]);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await { set_db.set(Some(d)); }
        });
    });

    let reload_all = {
        let db = db.clone();
        move || {
            if let Some(d) = db.get() {
                let d2 = d.clone(); let d3 = d.clone(); let d4 = d.clone(); let d5 = d.clone();
                spawn_local(async move { if let Ok(v) = d.get_all_weapons().await { set_weapons.set(v); } });
                spawn_local(async move { if let Ok(v) = d2.get_all_amps().await { set_amps.set(v); } });
                spawn_local(async move { if let Ok(v) = d3.get_all_armors().await { set_armors.set(v); } });
                spawn_local(async move { if let Ok(v) = d4.get_all_faps().await { set_faps.set(v); } });
                spawn_local(async move { if let Ok(v) = d5.get_all_loadouts().await { set_loadouts.set(v); } });
            }
        }
    };
    Effect::new({ let ra = reload_all.clone(); move |_| { let _ = db.get(); ra(); } });

    view! {
        <div class="space-y-4 max-w-3xl ">
            <h2 class="text-lg font-semibold ">"🔫 Loadouts"</h2>

            // Sub-Tabs
            <div class="flex gap-1 border-b ">
                {[
                    (SubTab::Weapons, "Waffen"),
                    (SubTab::Amps, "Amplifier"),
                    (SubTab::Armors, "Rüstungen"),
                    (SubTab::Faps, "FAPs"),
                    (SubTab::Loadouts, "Loadouts"),
                ].iter().map(|&(tab, label)| {
                    view! {
                        <button
                            class=move || if active.get() == tab {
                                "px-3 py-1 text-sm border-b-2 border-blue-500 text-blue-600"
                            } else {
                                "px-3 py-1 text-sm text-gray-600 hover:text-gray-900"
                            }
                            on:click=move |_| active.set(tab)
                        >{label}</button>
                    }
                }).collect_view()}
            </div>

            // Inhalt per Sub-Tab
            <Show when=move || active.get() == SubTab::Weapons>
                <WeaponCrud items=move || weapons.get() db=move || db.get() reload=reload_all.clone() />
            </Show>
            <Show when=move || active.get() == SubTab::Amps>
                <AmpCrud items=move || amps.get() db=move || db.get() reload=reload_all.clone() />
            </Show>
            <Show when=move || active.get() == SubTab::Armors>
                <ArmorCrud items=move || armors.get() db=move || db.get() reload=reload_all.clone() />
            </Show>
            <Show when=move || active.get() == SubTab::Faps>
                <FapCrud items=move || faps.get() db=move || db.get() reload=reload_all.clone() />
            </Show>
            <Show when=move || active.get() == SubTab::Loadouts>
                <LoadoutCrud items=move || loadouts.get() weapons=move || weapons.get() amps=move || amps.get() db=move || db.get() reload=reload_all.clone() />
            </Show>
        </div>
    }
}

// ─── WeaponCrud ───────────────────────────────────────────────────────────────

#[component]
fn WeaponCrud(
    items: impl Fn() -> Vec<Weapon> + 'static + Copy + Send,
    db: impl Fn() -> Option<Db> + 'static + Copy + Send,
    reload: impl Fn() + 'static + Clone + Send,
) -> impl IntoView {
    let name  = RwSignal::new(String::new());
    let dmin  = RwSignal::new(String::new());
    let dmax  = RwSignal::new(String::new());
    let pec   = RwSignal::new(String::new());

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let w = Weapon {
                id: Uuid::new_v4().to_string(),
                name: name.get_untracked(),
                damage_min: dmin.get_untracked().parse().unwrap_or(0.0),
                damage_max: dmax.get_untracked().parse().unwrap_or(0.0),
                pec_per_shot: pec.get_untracked().parse().unwrap_or(0.0),
            };
            if w.name.is_empty() { return; }
            if let Some(d) = db() {
                let reload = reload.clone();
                spawn_local(async move { let _ = d.save_weapon(&w).await; reload(); });
            }
        }
    };

    view! {
        <div class="space-y-3 ">
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <Field label="Name" sig=name />
                <Field label="Dmg Min" sig=dmin input_type="number" />
                <Field label="Dmg Max" sig=dmax input_type="number" />
                <Field label="PEC/Shot" sig=pec input_type="number" />
                <button class="btn-primary self-end " on:click=on_add>"+ Hinzufügen"</button>
            </div>
            <ItemList
                items=items
                key_fn=|w: &Weapon| w.id.clone()
                render=|w: Weapon| format!("{} | Dmg: {:.0}-{:.0} | {:.2} PEC/Shot", w.name, w.damage_min, w.damage_max, w.pec_per_shot)
                on_delete=move |id: String| {
                    if let Some(d) = db() {
                        let reload = reload.clone();
                        spawn_local(async move { let _ = d.delete_weapon(&id).await; reload(); });
                    }
                }
            />
        </div>
    }
}

// ─── AmpCrud ─────────────────────────────────────────────────────────────────

#[component]
fn AmpCrud(
    items: impl Fn() -> Vec<Amplifier> + 'static + Copy + Send,
    db: impl Fn() -> Option<Db> + 'static + Copy + Send,
    reload: impl Fn() + 'static + Clone + Send,
) -> impl IntoView {
    let name   = RwSignal::new(String::new());
    let bonus  = RwSignal::new(String::new());
    let decay  = RwSignal::new(String::new());

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let a = Amplifier {
                id: Uuid::new_v4().to_string(),
                name: name.get_untracked(),
                flat_damage_bonus: bonus.get_untracked().parse().unwrap_or(0.0),
                decay_pec_per_shot: decay.get_untracked().parse().unwrap_or(0.0),
            };
            if a.name.is_empty() { return; }
            if let Some(d) = db() {
                let reload = reload.clone();
                spawn_local(async move { let _ = d.save_amp(&a).await; reload(); });
            }
        }
    };

    view! {
        <div class="space-y-3 ">
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <Field label="Name" sig=name />
                <Field label="Dmg-Bonus" sig=bonus input_type="number" />
                <Field label="Decay PEC/Shot" sig=decay input_type="number" />
                <button class="btn-primary self-end " on:click=on_add>"+ Hinzufügen"</button>
            </div>
            <ItemList
                items=items
                key_fn=|a: &Amplifier| a.id.clone()
                render=|a: Amplifier| format!("{} | +{:.1} Dmg | {:.2} PEC Decay", a.name, a.flat_damage_bonus, a.decay_pec_per_shot)
                on_delete=move |id: String| {
                    if let Some(d) = db() {
                        let reload = reload.clone();
                        spawn_local(async move { let _ = d.delete_amp(&id).await; reload(); });
                    }
                }
            />
        </div>
    }
}

// ─── ArmorCrud ────────────────────────────────────────────────────────────────

#[component]
fn ArmorCrud(
    items: impl Fn() -> Vec<ArmorProfile> + 'static + Copy + Send,
    db: impl Fn() -> Option<Db> + 'static + Copy + Send,
    reload: impl Fn() + 'static + Clone + Send,
) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let pec  = RwSignal::new(String::new());

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let a = ArmorProfile {
                id: Uuid::new_v4().to_string(),
                name: name.get_untracked(),
                repair_pec_per_damage_point: pec.get_untracked().parse().unwrap_or(0.0),
            };
            if a.name.is_empty() { return; }
            if let Some(d) = db() {
                let reload = reload.clone();
                spawn_local(async move { let _ = d.save_armor(&a).await; reload(); });
            }
        }
    };

    view! {
        <div class="space-y-3 ">
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <Field label="Name" sig=name />
                <Field label="PEC/Schadenspunkt" sig=pec input_type="number" />
                <button class="btn-primary self-end " on:click=on_add>"+ Hinzufügen"</button>
            </div>
            <ItemList
                items=items
                key_fn=|a: &ArmorProfile| a.id.clone()
                render=|a: ArmorProfile| format!("{} | {:.4} PEC/Dmg", a.name, a.repair_pec_per_damage_point)
                on_delete=move |id: String| {
                    if let Some(d) = db() {
                        let reload = reload.clone();
                        spawn_local(async move { let _ = d.delete_armor(&id).await; reload(); });
                    }
                }
            />
        </div>
    }
}

// ─── FapCrud ─────────────────────────────────────────────────────────────────

#[component]
fn FapCrud(
    items: impl Fn() -> Vec<FapProfile> + 'static + Copy + Send,
    db: impl Fn() -> Option<Db> + 'static + Copy + Send,
    reload: impl Fn() + 'static + Clone + Send,
) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let pec  = RwSignal::new(String::new());

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let f = FapProfile {
                id: Uuid::new_v4().to_string(),
                name: name.get_untracked(),
                pec_per_heal_point: pec.get_untracked().parse().unwrap_or(0.0),
            };
            if f.name.is_empty() { return; }
            if let Some(d) = db() {
                let reload = reload.clone();
                spawn_local(async move { let _ = d.save_fap(&f).await; reload(); });
            }
        }
    };

    view! {
        <div class="space-y-3 ">
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <Field label="Name" sig=name />
                <Field label="PEC/Heilpunkt" sig=pec input_type="number" />
                <button class="btn-primary self-end " on:click=on_add>"+ Hinzufügen"</button>
            </div>
            <ItemList
                items=items
                key_fn=|f: &FapProfile| f.id.clone()
                render=|f: FapProfile| format!("{} | {:.4} PEC/HP", f.name, f.pec_per_heal_point)
                on_delete=move |id: String| {
                    if let Some(d) = db() {
                        let reload = reload.clone();
                        spawn_local(async move { let _ = d.delete_fap(&id).await; reload(); });
                    }
                }
            />
        </div>
    }
}

// ─── LoadoutCrud ──────────────────────────────────────────────────────────────

#[component]
fn LoadoutCrud(
    items: impl Fn() -> Vec<Loadout> + 'static + Copy + Send,
    weapons: impl Fn() -> Vec<Weapon> + 'static + Copy + Send,
    amps: impl Fn() -> Vec<Amplifier> + 'static + Copy + Send,
    db: impl Fn() -> Option<Db> + 'static + Copy + Send,
    reload: impl Fn() + 'static + Clone + Send,
) -> impl IntoView {
    let name       = RwSignal::new(String::new());
    let weapon_id  = RwSignal::new(String::new());
    let amp_id     = RwSignal::new(String::new());
    let enhancers  = RwSignal::new(String::from("0"));

    let on_add = {
        let reload = reload.clone();
        move |_| {
            let wid = weapon_id.get_untracked();
            let n = name.get_untracked();
            if n.is_empty() || wid.is_empty() { return; }
            let aid = amp_id.get_untracked();
            let enh: u8 = enhancers.get_untracked().parse().unwrap_or(0).min(10);
            let l = Loadout {
                id: Uuid::new_v4().to_string(),
                name: n,
                weapon_id: wid,
                amp_id: if aid.is_empty() { None } else { Some(aid) },
                damage_enhancers: enh,
            };
            if let Some(d) = db() {
                let reload = reload.clone();
                spawn_local(async move { let _ = d.save_loadout(&l).await; reload(); });
            }
        }
    };

    view! {
        <div class="space-y-3 ">
            <div class="flex flex-wrap gap-2 items-end bg-gray-50 p-3 rounded border text-sm ">
                <Field label="Name" sig=name />
                <label class="flex flex-col gap-1 ">
                    "Waffe"
                    <select class="input " on:change=move |ev| weapon_id.set(event_target_value(&ev))>
                        <option value="">"- wählen -"</option>
                        {move || weapons().into_iter().map(|w| {
                            let id = w.id.clone(); let nm = w.name.clone();
                            view!{<option value=id>{nm}</option>}
                        }).collect_view()}
                    </select>
                </label>
                <label class="flex flex-col gap-1 ">
                    "Amplifier (optional)"
                    <select class="input " on:change=move |ev| amp_id.set(event_target_value(&ev))>
                        <option value="">"- keiner -"</option>
                        {move || amps().into_iter().map(|a| {
                            let id = a.id.clone(); let nm = a.name.clone();
                            view!{<option value=id>{nm}</option>}
                        }).collect_view()}
                    </select>
                </label>
                <label class="flex flex-col gap-1 ">
                    "Dmg Enhancer (0–10)"
                    <input
                        type="number" min="0" max="10" step="1"
                        class="input w-20 "
                        prop:value=move || enhancers.get()
                        on:input=move |ev| enhancers.set(event_target_value(&ev))
                    />
                </label>
                <button class="btn-primary self-end " on:click=on_add>"+ Hinzufügen"</button>
            </div>

            // Loadout-Zusammenfassung live
            {move || {
                let wid = weapon_id.get();
                let aid = amp_id.get();
                let enh: u8 = enhancers.get().parse().unwrap_or(0).min(10);
                let ws = weapons();
                let amps_v = amps();
                let w = ws.iter().find(|w| w.id == wid);
                let a = amps_v.iter().find(|a| a.id == aid);
                if let Some(w) = w {
                    let dummy_loadout = Loadout {
                        id: String::new(), name: String::new(),
                        weapon_id: wid.clone(), amp_id: if aid.is_empty() { None } else { Some(aid) },
                        damage_enhancers: enh,
                    };
                    let pec = dummy_loadout.total_pec_per_shot(w, a);
                    let avg = dummy_loadout.avg_damage(w, a);
                    let enh_label = if enh > 0 { format!(" (+{}×Enh)", enh) } else { String::new() };
                    view!{
                        <div class="text-xs bg-blue-50 p-2 rounded border border-blue-200 ">
                            {format!("Ø Schaden: {:.1} pts{}  |  PEC/Shot: {:.2}{}",
                                avg, enh_label, pec,
                                if enh > 0 { format!(" (Waffe ×{:.1})", 1.0 + enh as f64 * 0.1) } else { String::new() })}
                        </div>
                    }.into_any()
                } else {
                    view!{<div></div>}.into_any()
                }
            }}

            <ItemList
                items=items
                key_fn=|l: &Loadout| l.id.clone()
                render=|l: Loadout| {
                    if l.damage_enhancers > 0 {
                        format!("{} ({}× Enh)", l.name, l.damage_enhancers)
                    } else {
                        l.name.clone()
                    }
                }
                on_delete=move |id: String| {
                    if let Some(d) = db() {
                        let reload = reload.clone();
                        spawn_local(async move { let _ = d.delete_loadout(&id).await; reload(); });
                    }
                }
            />
        </div>
    }
}

// ─── Generische Hilfkomponenten ───────────────────────────────────────────────

#[component]
fn Field(
    label: &'static str,
    sig: RwSignal<String>,
    #[prop(default = "text")] input_type: &'static str,
) -> impl IntoView {
    view! {
        <label class="flex flex-col gap-1 ">
            {label}
            <input class="input w-32 " type=input_type step="0.0001"
                value=move || sig.get()
                on:input=move |ev| sig.set(event_target_value(&ev))
            />
        </label>
    }
}

#[component]
fn ItemList<T, KF, R>(
    items: impl Fn() -> Vec<T> + 'static + Copy + Send,
    key_fn: KF,
    render: R,
    on_delete: impl Fn(String) + 'static + Clone + Send,
) -> impl IntoView
where
    T: Clone + 'static + Send,
    KF: Fn(&T) -> String + 'static + Copy + Send,
    R: Fn(T) -> String + 'static + Copy + Send,
{
    view! {
        <div class="space-y-1 ">
            {move || items().into_iter().map(|item| {
                let key = key_fn(&item);
                let label = render(item.clone());
                let k2 = key.clone();
                let on_delete = on_delete.clone();
                view! {
                    <div class="flex items-center justify-between border rounded p-2 text-sm bg-white ">
                        <span>{label}</span>
                        <button class="btn-xs-danger "
                            on:click=move |_| on_delete(k2.clone())
                        >"🗑"</button>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}
