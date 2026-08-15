use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::domain::types::{LootItemConfig, Material, SavedRun, slugify};
use crate::persistence::idb::Db;
use crate::services::drop_index::build_drop_index;
use crate::services::run_service::now_sec;

// ─── Unified Row ─────────────────────────────────────────────────────────────

/// Eine Zeile in der Materialien-Tabelle: vereint Material-Store + LootItemConfig.
#[derive(Clone, PartialEq)]
struct MatRow {
    pub name:              String,
    pub material_id:       String,
    pub tt_value:          f64,
    pub mu_pct:            f64,
    pub droppers:          Vec<String>,
    pub in_material_store: bool,
    pub last_updated_sec:  u64,
    pub custom_group:      Option<String>,
}

fn build_rows(materials: &[Material], loot_cfgs: &[LootItemConfig]) -> Vec<MatRow> {
    let mat_ids: std::collections::HashMap<&str, &Material> =
        materials.iter().map(|m| (m.id.as_str(), m)).collect();
    let cfg_map: std::collections::HashMap<&str, &LootItemConfig> =
        loot_cfgs.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut rows: Vec<MatRow> = Vec::new();

    for mat in materials {
        let cfg = cfg_map.get(mat.name.as_str());
        rows.push(MatRow {
            name:              mat.name.clone(),
            material_id:       mat.id.clone(),
            tt_value:          mat.tt_value,
            mu_pct:            cfg.map(|c| c.mu_pct).unwrap_or(mat.markup_pct),
            droppers:          cfg.map(|c| c.droppers.clone()).unwrap_or_default(),
            in_material_store: true,
            last_updated_sec:  cfg.and_then(|c| c.last_updated_sec).unwrap_or(0),
            custom_group:      mat.group.clone(),
        });
    }

    for cfg in loot_cfgs {
        let mat_id = slugify(&cfg.name);
        if !mat_ids.contains_key(mat_id.as_str()) {
            rows.push(MatRow {
                name:              cfg.name.clone(),
                material_id:       mat_id,
                tt_value:          0.0,
                mu_pct:            cfg.mu_pct,
                droppers:          cfg.droppers.clone(),
                in_material_store: false,
                last_updated_sec:  cfg.last_updated_sec.unwrap_or(0),
                custom_group:      None,
            });
        }
    }

    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

pub fn categorize(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("residue") || n.contains("shrapnel") {
        "⚙️ Residue & Schrott"
    } else if n.contains("component") || n.contains("socket") || n.contains("tier")
           || n.contains("link") || n.contains("sink") || n.contains("cable")
           || n.contains("mounting") || n.contains("circuit") || n.contains("sensor")
           || n.contains("module") || n.contains("connector") || n.contains("regulator")
           || n.contains("suppressor") || n.contains("plug") || n.contains("coil") {
        "🔩 Komponenten"
    } else if n.contains("ingot") || n.contains("alloy") || n.contains("iron")
           || n.contains("steel") || n.contains("copper") || n.contains("aluminum")
           || n.contains("chrome") || n.contains("nickel") || n.contains("lead")
           || n.contains("tin") || n.contains("metal") || n.contains("ore")
           || n.contains("pellet") || n.contains("stone") || n.contains("gem")
           || n.contains("mineral") {
        "🪨 Metalle & Erze"
    } else if n.contains("oil") || n.contains("gas") || n.contains("fluid")
           || n.contains("liquid") || n.contains("sweat") || n.contains("dung")
           || n.contains("acid") || n.contains("solution") || n.contains("extract")
           || n.contains("gel") || n.contains("wax") || n.contains("resin") {
        "🌊 Enmatter"
    } else if n.contains("treasure") || n.contains("relic") || n.contains("artifact")
           || n.contains("ancient") || n.contains("fragment") || n.contains("ruin")
           || n.contains("shard") || n.contains("token") || n.contains("tablet") {
        "💎 Treasure Parts"
    } else if n.contains("paint") || n.contains("dye") || n.contains("color")
           || n.contains("pigment") {
        "🎨 Farben"
    } else if n.contains("leather") || n.contains("fiber") || n.contains("cloth")
           || n.contains("hide") || n.contains("bone") || n.contains("wood")
           || n.contains("fruit") || n.contains("crystal") {
        "🌿 Rohstoffe"
    } else {
        "📦 Sonstige"
    }
}

// ─── Sync-Helper: MU% in beiden Stores speichern ─────────────────────────────

async fn sync_mu(db: &Db, row: &MatRow, new_mu: f64, new_tt: Option<f64>) {
    // LootItemConfig
    let cfg = LootItemConfig {
        name:             row.name.clone(),
        mu_pct:           new_mu,
        droppers:         row.droppers.clone(),
        last_updated_sec: Some(now_sec()),
    };
    let _ = db.save_loot_item_config(&cfg).await;

    // Material-Store
    if row.in_material_store {
        let tt = new_tt.unwrap_or(row.tt_value);
        let mat = Material { id: row.material_id.clone(), name: row.name.clone(),
            tt_value: tt, markup_pct: new_mu, group: row.custom_group.clone() };
        let _ = db.save_material(&mat).await;
    } else if let Some(tt) = new_tt {
        let mat = Material { id: row.material_id.clone(), name: row.name.clone(),
            tt_value: tt, markup_pct: new_mu, group: row.custom_group.clone() };
        let _ = db.save_material(&mat).await;
    }
}

// ─── Haupt-Komponente ─────────────────────────────────────────────────────────

#[component]
pub fn MuPanel() -> impl IntoView {
    let (db, set_db)           = signal_local::<Option<Db>>(None);
    let (materials, set_mats)  = signal_local::<Vec<Material>>(vec![]);
    let (loot_cfgs, set_cfgs)  = signal_local::<Vec<LootItemConfig>>(vec![]);
    let (runs, set_runs)       = signal_local::<Vec<SavedRun>>(vec![]);
    let show_add_form          = RwSignal::new(false);
    let search_query           = RwSignal::new(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(d) = Db::open().await { set_db.set(Some(d)); }
        });
    });

    let reload = {
        let db = db.clone();
        move || {
            if let Some(d) = db.get() {
                let (d2, d3) = (d.clone(), d.clone());
                spawn_local(async move {
                    if let Ok(v) = d.get_all_materials().await { set_mats.set(v); }
                });
                spawn_local(async move {
                    if let Ok(v) = d2.get_all_loot_item_configs().await {
                        let mut s = v; s.sort_by(|a,b| a.name.cmp(&b.name));
                        set_cfgs.set(s);
                    }
                });
                spawn_local(async move {
                    if let Ok(v) = d3.get_all_runs().await { set_runs.set(v); }
                });
            }
        }
    };

    Effect::new({ let r = reload.clone(); move |_| { let _ = db.get(); r(); } });

    // ── OCR-State ────────────────────────────────────────────────────────────
    let ocr_item_name  = RwSignal::new(String::new());
    let ocr_image_data = RwSignal::new(String::new());
    let ocr_running    = RwSignal::new(false);
    let ocr_raw_text   = RwSignal::new(String::new());
    let ocr_mus_str    = RwSignal::new(String::new());
    let ocr_error      = RwSignal::new(String::new());

    let on_image_pick = move |ev: leptos::ev::Event| {
        let input = ev.target().unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
        if let Some(file) = input.files().and_then(|fl| fl.get(0)) {
            let file_js: wasm_bindgen::JsValue = file.into();
            ocr_mus_str.set(String::new());
            ocr_raw_text.set(String::new());
            ocr_error.set(String::new());
            spawn_local(async move {
                let blob: web_sys::Blob = file_js.dyn_into().unwrap();
                let reader = web_sys::FileReader::new().unwrap();
                let rc = reader.clone();
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    let rc2 = rc.clone();
                    let cb = wasm_bindgen::closure::Closure::once(move || {
                        let _ = resolve.call1(&wasm_bindgen::JsValue::NULL, &rc2.result().unwrap());
                    });
                    rc.set_onloadend(Some(cb.as_ref().unchecked_ref()));
                    cb.forget();
                });
                reader.read_as_data_url(&blob).unwrap();
                let val = JsFuture::from(promise).await.unwrap_or_default();
                ocr_image_data.set(val.as_string().unwrap_or_default());
            });
        }
    };

    let on_ocr_start = move |_| {
        let data_url = ocr_image_data.get_untracked();
        if data_url.is_empty() { return; }
        ocr_running.set(true);
        ocr_error.set(String::new());
        spawn_local(async move {
            let window = web_sys::window().unwrap();
            match js_sys::Reflect::get(&wasm_bindgen::JsValue::from(window),
                                       &wasm_bindgen::JsValue::from_str("ocrImage")) {
                Ok(v) => match v.dyn_into::<js_sys::Function>() {
                    Ok(f) => {
                        let arg = wasm_bindgen::JsValue::from_str(&data_url);
                        match f.call1(&wasm_bindgen::JsValue::NULL, &arg) {
                            Ok(p) => match JsFuture::from(js_sys::Promise::from(p)).await {
                                Ok(t) => {
                                    let text = t.as_string().unwrap_or_default();
                                    let mus = parse_auction_ocr(&text);
                                    ocr_raw_text.set(text);
                                    ocr_mus_str.set(mus.iter().map(|v| format!("{v:.1}")).collect::<Vec<_>>().join(","));
                                }
                                Err(e) => ocr_error.set(format!("OCR-Fehler: {e:?}")),
                            },
                            Err(e) => ocr_error.set(format!("Aufruf-Fehler: {e:?}")),
                        }
                    }
                    Err(_) => ocr_error.set("ocrImage ist keine Funktion".into()),
                },
                Err(_) => ocr_error.set("ocrImage nicht gefunden – Internetverbindung prüfen.".into()),
            }
            ocr_running.set(false);
        });
    };

    view! {
        <div class="space-y-4 max-w-4xl ">
            <div class="flex items-center justify-between flex-wrap gap-2 ">
                <h2 class="text-lg font-semibold ">"📦 Materialien & Markup"</h2>
                <div class="flex items-center gap-2 ">
                    <input
                        class="input w-48 "
                        type="search"
                        placeholder="Suchen…"
                        prop:value=move || search_query.get()
                        on:input=move |ev| search_query.set(event_target_value(&ev))
                    />
                    <button class="btn-primary "
                        on:click=move |_| show_add_form.update(|v| *v = !*v)>
                        {move || if show_add_form.get() { "✕ Abbrechen" } else { "+ Manuell" }}
                    </button>
                </div>
            </div>
            <p class="text-sm text-gray-500 ">
                "Nexus-importierte Materialien erscheinen automatisch. "
                "Loot-Items werden beim Hunt-Run erfasst. "
                "MU%-Änderungen werden in beiden Quellen synchron gespeichert."
            </p>

            // ── Manuell hinzufügen ────────────────────────────────────────────
            <Show when=move || show_add_form.get()>
                <AddMaterialForm
                    db=move || db.get()
                    on_done=move || { show_add_form.set(false); reload(); }
                />
            </Show>

            // ── OCR-Preisimport ───────────────────────────────────────────────
            <details class="border rounded bg-blue-50 border-blue-200 ">
                <summary class="p-3 cursor-pointer font-semibold text-blue-800 ">
                    "🔍 OCR-Preisimport (EU-Auktion)"
                </summary>
                <div class="p-4 space-y-3 ">
                    <p class="text-xs text-blue-700 ">
                        "EU-Auktion öffnen → Item suchen → Bildausschnitt der ersten 10 Reihen hochladen."
                    </p>
                    <div class="flex gap-3 flex-wrap items-end ">
                        <label class="flex flex-col gap-1 text-sm ">
                            "Item-Name"
                            <input class="input w-48 " placeholder="z.B. Banite Ingot"
                                prop:value=move || ocr_item_name.get()
                                on:input=move |ev| ocr_item_name.set(event_target_value(&ev)) />
                        </label>
                        <label class="flex flex-col gap-1 text-sm ">
                            "Screenshot"
                            <input type="file" accept="image/*" on:change=on_image_pick />
                        </label>
                    </div>
                    {move || {
                        let data = ocr_image_data.get();
                        if data.is_empty() { return view!{<span></span>}.into_any(); }
                        view! {
                            <div class="space-y-2 ">
                                <img src=data class="max-h-40 border rounded " alt="Vorschau" />
                                <button class="btn-primary " disabled=move || ocr_running.get()
                                    on:click=on_ocr_start.clone()>
                                    {move || if ocr_running.get() { "⏳ OCR läuft…" } else { "🔍 OCR starten" }}
                                </button>
                            </div>
                        }.into_any()
                    }}
                    {move || { let e = ocr_error.get(); if e.is_empty() { return view!{<span></span>}.into_any(); }
                        view!{<div class="text-sm text-red-600 ">{e}</div>}.into_any()
                    }}
                    {move || {
                        let mus_str = ocr_mus_str.get();
                        if mus_str.is_empty() { return view!{<span></span>}.into_any(); }
                        let mus: Vec<f64> = mus_str.split(',').filter_map(|s| s.parse().ok()).collect();
                        if mus.is_empty() {
                            return view!{<div class="text-sm text-orange-600 ">"Keine MU%-Werte erkannt."</div>}.into_any();
                        }
                        let min_mu = mus.iter().cloned().fold(f64::MAX, f64::min);
                        let median = { let mut s = mus.clone(); s.sort_by(|a,b| a.partial_cmp(b).unwrap()); s[s.len()/2] };
                        let mus_list = mus.iter().map(|v| format!("{v:.1}%")).collect::<Vec<_>>().join("  ·  ");
                        let item = ocr_item_name.get_untracked();

                        let save_ocr_mu = {
                            let item = item.clone();
                            let db   = db.clone();
                            let mats = materials.clone();
                            let cfgs = loot_cfgs.clone();
                            let reload2 = reload.clone();
                            move |mu: f64| {
                                if item.is_empty() { return; }
                                if let Some(d) = db.get_untracked() {
                                    let rows = build_rows(&mats.get_untracked(), &cfgs.get_untracked());
                                    let row  = rows.into_iter().find(|r| r.name == item)
                                        .unwrap_or_else(|| MatRow {
                                            name: item.clone(), material_id: slugify(&item),
                                            tt_value: 0.0, mu_pct: mu,
                                            droppers: vec![], in_material_store: false,
                                            last_updated_sec: 0, custom_group: None,
                                        });
                                    let reload3 = reload2.clone();
                                    spawn_local(async move {
                                        sync_mu(&d, &row, mu, None).await;
                                        reload3();
                                    });
                                }
                            }
                        };
                        let save1 = { let s = save_ocr_mu.clone(); move |_| s(min_mu) };
                        let save2 = { let s = save_ocr_mu.clone(); move |_| s(median) };

                        view! {
                            <div class="bg-white border rounded p-3 space-y-2 ">
                                <div class="text-sm font-medium ">
                                    "Erkannte MU-Werte: "
                                    <span class="font-mono text-blue-700 ">{mus_list}</span>
                                </div>
                                <div class="flex gap-6 text-sm ">
                                    <span>"Min: "<strong class="text-green-700 ">{format!("{min_mu:.1}%")}</strong></span>
                                    <span>"Median: "<strong>{format!("{median:.1}%")}</strong></span>
                                </div>
                                <div class="flex gap-2 flex-wrap ">
                                    <button class="btn-primary text-sm " on:click=save1>
                                        {format!("✅ {min_mu:.1}% übernehmen (niedrigstes)")}
                                    </button>
                                    <button class="btn-secondary text-sm " on:click=save2>
                                        {format!("📊 {median:.1}% (Median)")}
                                    </button>
                                </div>
                                <details class="text-xs ">
                                    <summary class="cursor-pointer text-gray-400 ">"OCR-Rohtext"</summary>
                                    <pre class="mt-1 bg-gray-50 border rounded p-2 overflow-auto max-h-32 whitespace-pre-wrap ">
                                        {move || ocr_raw_text.get()}
                                    </pre>
                                </details>
                            </div>
                        }.into_any()
                    }}
                </div>
            </details>

            // ── Materialien-Tabelle (gruppiert) ──────────────────────────────
            {move || {
                let q        = search_query.get().to_lowercase();
                let all_rows = build_rows(&materials.get(), &loot_cfgs.get());
                let drop_idx = build_drop_index(&runs.get());

                if all_rows.is_empty() {
                    return view!{<p class="text-gray-400 italic ">
                        "Noch keine Materialien – Nexus importieren oder manuell hinzufügen."
                    </p>}.into_any();
                }

                // ── Suche: flache Liste ───────────────────────────────────────
                if !q.is_empty() {
                    let rows: Vec<_> = all_rows.into_iter()
                        .filter(|r| r.name.to_lowercase().contains(&q))
                        .collect();
                    if rows.is_empty() {
                        return view!{<p class="text-gray-400 italic ">"Keine Treffer."</p>}.into_any();
                    }
                    let db2 = move || db.get(); let rel2 = reload.clone(); let di2 = drop_idx.clone();
                    return view!{
                        <table class="w-full text-sm border-collapse ">
                            {table_head()}
                            <tbody>
                                {rows.into_iter().map(|row| view!{
                                    <MatRowView row=row db=db2 reload=rel2.clone() drop_idx=di2.clone() />
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any();
                }

                // ── Gruppierte Ansicht ────────────────────────────────────────
                const RECENT_MAX: usize = 50;
                let mut recent: Vec<_> = all_rows.iter()
                    .filter(|r| r.last_updated_sec > 0)
                    .cloned().collect();
                recent.sort_by(|a, b| b.last_updated_sec.cmp(&a.last_updated_sec));
                recent.truncate(RECENT_MAX);

                const CATS: &[&str] = &[
                    "🔩 Komponenten",
                    "⚙️ Residue & Schrott",
                    "🪨 Metalle & Erze",
                    "🌊 Enmatter",
                    "💎 Treasure Parts",
                    "🌿 Rohstoffe",
                    "🎨 Farben",
                    "📦 Sonstige",
                ];
                let mut groups: Vec<(&'static str, Vec<MatRow>)> = CATS.iter()
                    .map(|&cat| (cat, vec![]))
                    .collect();
                for row in &all_rows {
                    let cat = row.custom_group.as_deref().unwrap_or_else(|| categorize(&row.name));
                    if let Some(g) = groups.iter_mut().find(|(c, _)| *c == cat) {
                        g.1.push(row.clone());
                    } else {
                        // custom_group die keiner CATS entspricht → Sonstige
                        if let Some(g) = groups.iter_mut().find(|(c, _)| *c == "📦 Sonstige") {
                            g.1.push(row.clone());
                        }
                    }
                }

                let mk_section = {
                    let reload = reload.clone(); let drop_idx = drop_idx.clone();
                    move |title: &'static str, rows: Vec<MatRow>, open: bool| {
                        if rows.is_empty() { return view!{<div></div>}.into_any(); }
                        let count = rows.len();
                        let db2 = move || db.get(); let rel2 = reload.clone(); let di2 = drop_idx.clone();
                        view!{
                            <details class="border rounded " open=open>
                                <summary class="px-3 py-2 cursor-pointer font-medium text-sm select-none bg-gray-50 hover:bg-gray-100 ">
                                    {title}" "
                                    <span class="text-gray-400 font-normal text-xs">"("{count}")"</span>
                                </summary>
                                <table class="w-full text-sm border-collapse ">
                                    {table_head()}
                                    <tbody>
                                        {rows.into_iter().map(|row| view!{
                                            <MatRowView row=row db=db2 reload=rel2.clone() drop_idx=di2.clone() />
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </details>
                        }.into_any()
                    }
                };

                let recent_count = recent.len();
                view!{
                    <div class="space-y-2 ">
                        {if recent_count > 0 {
                            mk_section("🕐 Zuletzt aktiv", recent, true)
                        } else {
                            view!{<div></div>}.into_any()
                        }}
                        {groups.into_iter().map(|(cat, rows)| mk_section(cat, rows, false)).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

async fn save_group(db: &Db, row: &MatRow, group: Option<String>) {
    let mat = Material {
        id: row.material_id.clone(), name: row.name.clone(),
        tt_value: row.tt_value, markup_pct: row.mu_pct, group,
    };
    let _ = db.save_material(&mat).await;
}

const GROUP_OPTIONS: &[&str] = &[
    "(Auto)",
    "🔩 Komponenten",
    "⚙️ Residue & Schrott",
    "🪨 Metalle & Erze",
    "🌊 Enmatter",
    "💎 Treasure Parts",
    "🌿 Rohstoffe",
    "🎨 Farben",
    "📦 Sonstige",
];

// ─── Einzelne Tabellenzeile ───────────────────────────────────────────────────

#[component]
fn MatRowView(
    row: MatRow,
    db: impl Fn() -> Option<Db> + 'static + Copy,
    reload: impl Fn() + 'static + Clone,
    drop_idx: crate::domain::types::DropIndex,
) -> impl IntoView {
    use crate::services::drop_index::best_sources_for;

    let best_source = {
        let mut src = best_sources_for(&row.name, &drop_idx);
        if src.is_empty() {
            src = row.droppers.iter().map(|c| (c.clone(),
                crate::domain::types::DropStats { drop_rate: 0.0, avg_tt_per_run: 0.0 }
            )).collect();
        }
        src.into_iter().take(2).map(|(c, s)| {
            if s.drop_rate > 0.0 { format!("{c} ({:.0}%)", s.drop_rate * 100.0) } else { c }
        }).collect::<Vec<_>>().join(", ")
    };
    let best_source_display = if best_source.is_empty() { "–".to_string() } else { best_source };

    let mu_sig    = RwSignal::new(format!("{:.1}", row.mu_pct));
    let tt_sig    = RwSignal::new(format!("{:.4}", row.tt_value));
    let group_sig = RwSignal::new(row.custom_group.clone().unwrap_or_else(|| "(Auto)".to_string()));
    let row_s     = StoredValue::new(row.clone());
    let is_nexus   = row.in_material_store;
    let name_del   = row.name.clone();
    let mat_id_del = row.material_id.clone();
    let reload2 = reload.clone();
    let reload3 = reload.clone();
    let reload4 = reload.clone();

    view! {
        <tr class="border-b hover:bg-gray-50 ">
            <td class="p-2 font-medium ">
                {row.name.clone()}
                {if !is_nexus {
                    view!{<span class="ml-1 text-xs text-gray-400 ">"(Hunt)"</span>}.into_any()
                } else { view!{<span></span>}.into_any() }}
            </td>
            <td class="p-2 text-right ">
                <input class="input w-24 text-right "
                    type="number" step="0.0001" min="0"
                    disabled=move || !row_s.get_value().in_material_store
                    value=move || tt_sig.get()
                    on:input=move |ev| tt_sig.set(event_target_value(&ev)) />
            </td>
            <td class="p-2 text-right ">
                <input class="input w-24 text-right "
                    type="number" step="0.1" min="100"
                    value=move || mu_sig.get()
                    on:input=move |ev| mu_sig.set(event_target_value(&ev)) />
            </td>
            <td class="p-2 text-right font-mono text-xs ">
                {move || {
                    let tt: f64 = tt_sig.get().parse().unwrap_or(row_s.get_value().tt_value);
                    let mu: f64 = mu_sig.get().parse().unwrap_or(row_s.get_value().mu_pct);
                    format!("{:.4}", tt * mu / 100.0)
                }}
            </td>
            <td class="p-2 text-xs text-gray-500 ">{best_source_display}</td>
            <td class="p-2 ">
                <select class="input text-xs py-0.5 "
                    on:change=move |ev| {
                        let val = event_target_value(&ev);
                        group_sig.set(val.clone());
                        let group = if val == "(Auto)" { None } else { Some(val) };
                        let row = row_s.get_value();
                        if let Some(d) = db() {
                            let r4 = reload4.clone();
                            spawn_local(async move { save_group(&d, &row, group).await; r4(); });
                        }
                    }>
                    {GROUP_OPTIONS.iter().map(|&opt| {
                        let selected = move || group_sig.get() == opt;
                        view!{ <option value=opt selected=selected>{opt}</option> }
                    }).collect_view()}
                </select>
            </td>
            <td class="p-2 text-right space-x-1 ">
                <button class="btn-xs " on:click=move |_| {
                    let mu: f64 = mu_sig.get_untracked().parse().unwrap_or(100.0);
                    let tt: Option<f64> = tt_sig.get_untracked().parse().ok();
                    let row = row_s.get_value();
                    if let Some(d) = db() {
                        let r2 = reload2.clone();
                        spawn_local(async move { sync_mu(&d, &row, mu, tt).await; r2(); });
                    }
                }>"💾"</button>
                <button class="btn-xs-danger " on:click=move |_| {
                    if let Some(d) = db() {
                        let nm  = name_del.clone();
                        let mid = mat_id_del.clone();
                        let r3  = reload3.clone();
                        spawn_local(async move {
                            let _ = d.delete_loot_item_config(&nm).await;
                            let _ = d.delete_material(&mid).await;
                            r3();
                        });
                    }
                }>"🗑"</button>
            </td>
        </tr>
    }
}

fn table_head() -> impl IntoView {
    view! {
        <thead class="border-b bg-gray-50 sticky top-0 ">
            <tr>
                <th class="text-left p-2 ">"Material"</th>
                <th class="text-right p-2 ">"TT (PED)"</th>
                <th class="text-right p-2 ">"MU%"</th>
                <th class="text-right p-2 ">"Marktpreis"</th>
                <th class="text-left p-2 text-gray-500 text-xs ">"Beste Quelle"</th>
                <th class="text-left p-2 text-gray-500 text-xs ">"Gruppe"</th>
                <th class="p-2 "></th>
            </tr>
        </thead>
    }
}

// ─── Manuell hinzufügen ───────────────────────────────────────────────────────

#[component]
fn AddMaterialForm<F: Fn() + 'static + Clone>(
    db: impl Fn() -> Option<Db> + 'static + Copy,
    on_done: F,
) -> impl IntoView {
    let name   = RwSignal::new(String::new());
    let tt     = RwSignal::new(String::new());
    let mu     = RwSignal::new(String::from("100"));
    let status = RwSignal::new(String::new());

    let on_save = {
        let on_done = on_done.clone();
        move |_| {
            let n = name.get_untracked();
            if n.is_empty() { return; }
            let tt_val: f64 = tt.get_untracked().parse().unwrap_or(0.0);
            let mu_val: f64 = mu.get_untracked().parse().unwrap_or(100.0);
            if let Some(d) = db() {
                let on_done = on_done.clone();
                spawn_local(async move {
                    let mat = Material {
                        id: slugify(&n), name: n.clone(),
                        tt_value: tt_val, markup_pct: mu_val, group: None,
                    };
                    if let Err(e) = d.save_material(&mat).await {
                        status.set(format!("✗ {e:?}"));
                        return;
                    }
                    on_done();
                });
            }
        }
    };

    view! {
        <div class="border rounded p-4 bg-white space-y-3 text-sm ">
            <h3 class="font-medium ">"Neues Material anlegen"</h3>
            <div class="grid gap-2 sm:grid-cols-3 ">
                <label class="flex flex-col gap-1 ">"Name"
                    <input class="input " placeholder="z.B. Banite Ingot"
                        value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"TT-Wert (PED/Unit)"
                    <input class="input " type="number" step="0.0001" min="0"
                        value=move || tt.get()
                        on:input=move |ev| tt.set(event_target_value(&ev)) />
                </label>
                <label class="flex flex-col gap-1 ">"Markup %"
                    <input class="input " type="number" step="1" min="100"
                        value=move || mu.get()
                        on:input=move |ev| mu.set(event_target_value(&ev)) />
                </label>
            </div>
            {move || { let s = status.get(); if s.is_empty() { return view!{<span></span>}.into_any(); }
                view!{<p class="text-red-600 text-xs ">{s}</p>}.into_any()
            }}
            <div class="space-x-2 ">
                <button class="btn-primary " on:click=on_save>"💾 Speichern"</button>
                <button class="btn-secondary " on:click=move |_| on_done.clone()()>"Abbrechen"</button>
            </div>
        </div>
    }
}

// ─── OCR-Parser ───────────────────────────────────────────────────────────────

fn parse_auction_ocr(text: &str) -> Vec<f64> {
    let mut results = vec![];
    for line in text.lines() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_digit() {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
                let num_end = i;
                while i < bytes.len() && bytes[i] == b' ' { i += 1; }
                if i < bytes.len() && bytes[i] == b'%' {
                    if let Ok(v) = line[start..num_end].parse::<f64>() {
                        if v >= 100.0 && v <= 9999.0 { results.push(v); }
                    }
                    i += 1;
                }
            } else { i += 1; }
        }
    }
    if results.is_empty() {
        for line in text.lines() {
            let nums = extract_decimals(line);
            for w in nums.windows(2) {
                let (a, b) = (w[0], w[1]);
                if b > 0.1 && a > b { let mu = a/b*100.0; if mu>=100.0&&mu<=5000.0 { results.push(mu); break; } }
                else if a > 0.1 && b > a { let mu = b/a*100.0; if mu>=100.0&&mu<=5000.0 { results.push(mu); break; } }
            }
        }
    }
    results
}

fn extract_decimals(s: &str) -> Vec<f64> {
    let mut out = vec![];
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
            if let Ok(v) = s[start..i].parse::<f64>() { out.push(v); }
        } else { i += 1; }
    }
    out
}
