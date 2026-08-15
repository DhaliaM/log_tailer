use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

use crate::domain::types::{
    Amplifier, ArmorProfile, Blueprint, CreatureConfig, FapProfile, InventoryEntry,
    Loadout, LootItemConfig, Material, MuMap, PurchaseRecord, SaleRecord, SavedRun, Weapon,
};
use crate::persistence::idb::Db;

// ─── Backup-Struktur ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    pub player_name: String,
    pub runs: Vec<SavedRun>,
    pub creatures: Vec<CreatureConfig>,
    pub weapons: Vec<Weapon>,
    pub amps: Vec<Amplifier>,
    pub armor_profiles: Vec<ArmorProfile>,
    pub fap_profiles: Vec<FapProfile>,
    pub loadouts: Vec<Loadout>,
    pub blueprints: Vec<Blueprint>,
    pub mu_map: MuMap,
    // v4: Crafting-Erweiterung
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub loot_item_configs: Vec<LootItemConfig>,
    #[serde(default)]
    pub inventory: Vec<InventoryEntry>,
    #[serde(default)]
    pub sales: Vec<SaleRecord>,
    #[serde(default)]
    pub purchases: Vec<PurchaseRecord>,
}

// ─── Export ───────────────────────────────────────────────────────────────────

pub async fn export_all(db: &Db) -> Result<String, JsValue> {
    let player_name = db.get_setting("player_name").await?.unwrap_or_default();
    let mu_map_json = db.get_setting("mu_map").await?.unwrap_or_else(|| "{}".into());
    let mu_map: MuMap = serde_json::from_str(&mu_map_json).unwrap_or_default();

    let backup = Backup {
        version: 2,
        player_name,
        runs:            db.get_all_runs().await?,
        creatures:       db.get_all_creatures().await?,
        weapons:         db.get_all_weapons().await?,
        amps:            db.get_all_amps().await?,
        armor_profiles:  db.get_all_armors().await?,
        fap_profiles:    db.get_all_faps().await?,
        loadouts:        db.get_all_loadouts().await?,
        blueprints:      db.get_all_blueprints().await?,
        mu_map,
        materials:        db.get_all_materials().await?,
        loot_item_configs: db.get_all_loot_item_configs().await?,
        inventory:        db.get_all_inventory().await?,
        sales:            db.get_all_sales().await?,
        purchases:        db.get_all_purchases().await?,
    };

    serde_json::to_string_pretty(&backup).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Exportiert alle gespeicherten Runs als CSV-String.
pub async fn export_runs_csv(db: &Db) -> Result<String, JsValue> {
    let runs = db.get_all_runs().await?;
    let mut out = String::new();
    out.push_str("id,started_at,stopped_at,creature,loadout_id,kills,total_shots,total_loot_tt,ammo_cost_ped,armor_decay_ped,fap_decay_ped,total_cost_ped,return_pct_tt,profit_ped,globals_count\n");
    for r in &runs {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.2},{:.4},{}\n",
            r.id,
            r.started_at,
            r.stopped_at,
            r.config.creature,
            r.config.loadout_id,
            r.stats.kills,
            r.stats.total_shots,
            r.stats.total_loot_value_ped,
            r.ammo_cost_ped,
            r.armor_decay_ped,
            r.fap_decay_ped,
            r.total_cost_ped,
            r.return_pct_tt,
            r.profit_ped,
            r.stats.globals.len(),
        ));
    }
    Ok(out)
}

/// Triggert einen Browser-Download der JSON-Datei.
pub fn trigger_download(filename: &str, json: &str) {
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};
    use js_sys::Array;
    use wasm_bindgen::JsCast;

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let arr = Array::new();
    arr.push(&JsValue::from_str(json));

    let opts = BlobPropertyBag::new();
    opts.set_type("application/json");
    let blob = Blob::new_with_str_sequence_and_options(&arr, &opts).unwrap();
    let url = Url::create_object_url_with_blob(&blob).unwrap();

    let anchor: HtmlAnchorElement = document
        .create_element("a").unwrap()
        .dyn_into().unwrap();
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    Url::revoke_object_url(&url).unwrap_or_default();
}

/// Triggert einen Browser-Download mit angegebenem MIME-Typ.
pub fn trigger_download_text(filename: &str, content: &str, mime: &str) {
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};
    use js_sys::Array;
    use wasm_bindgen::JsCast;

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let arr = Array::new();
    arr.push(&JsValue::from_str(content));

    let opts = BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = Blob::new_with_str_sequence_and_options(&arr, &opts).unwrap();
    let url = Url::create_object_url_with_blob(&blob).unwrap();

    let anchor: HtmlAnchorElement = document
        .create_element("a").unwrap()
        .dyn_into().unwrap();
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    Url::revoke_object_url(&url).unwrap_or_default();
}

// ─── Import ───────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub enum ConflictStrategy {
    Overwrite,
    Skip,
}

pub struct ImportSummary {
    pub runs_imported: u64,
    pub creatures_imported: u64,
    pub weapons_imported: u64,
    pub amps_imported: u64,
    pub armors_imported: u64,
    pub faps_imported: u64,
    pub loadouts_imported: u64,
    pub blueprints_imported: u64,
    pub materials_imported: u64,
    pub sales_imported: u64,
    pub purchases_imported: u64,
}

pub async fn import_all(
    db: &Db,
    json: &str,
    strategy: ConflictStrategy,
) -> Result<ImportSummary, JsValue> {
    let backup: Backup = serde_json::from_str(json)
        .map_err(|e| JsValue::from_str(&format!("JSON parse error: {}", e)))?;

    // Für "Skip": erst bestehende IDs laden und filtern
    let (existing_run_ids, existing_creature_ids, existing_weapon_ids,
         existing_amp_ids, existing_armor_ids, existing_fap_ids,
         existing_loadout_ids, existing_blueprint_ids) = match strategy {
        ConflictStrategy::Skip => {
            let run_ids:      std::collections::HashSet<u64>     = db.get_all_runs().await?.iter().map(|r| r.id).collect();
            let creature_ids: std::collections::HashSet<String>  = db.get_all_creatures().await?.iter().map(|c| c.creature.clone()).collect();
            let weapon_ids:   std::collections::HashSet<String>  = db.get_all_weapons().await?.iter().map(|w| w.id.clone()).collect();
            let amp_ids:      std::collections::HashSet<String>  = db.get_all_amps().await?.iter().map(|a| a.id.clone()).collect();
            let armor_ids:    std::collections::HashSet<String>  = db.get_all_armors().await?.iter().map(|a| a.id.clone()).collect();
            let fap_ids:      std::collections::HashSet<String>  = db.get_all_faps().await?.iter().map(|f| f.id.clone()).collect();
            let loadout_ids:  std::collections::HashSet<String>  = db.get_all_loadouts().await?.iter().map(|l| l.id.clone()).collect();
            let bp_ids:       std::collections::HashSet<String>  = db.get_all_blueprints().await?.iter().map(|b| b.id.clone()).collect();
            (run_ids, creature_ids, weapon_ids, amp_ids, armor_ids, fap_ids, loadout_ids, bp_ids)
        }
        ConflictStrategy::Overwrite => Default::default(),
    };

    let mut summary = ImportSummary {
        runs_imported: 0, creatures_imported: 0, weapons_imported: 0,
        amps_imported: 0, armors_imported: 0, faps_imported: 0,
        loadouts_imported: 0, blueprints_imported: 0,
        materials_imported: 0, sales_imported: 0, purchases_imported: 0,
    };

    for r in backup.runs {
        if existing_run_ids.contains(&r.id) { continue; }
        db.save_run(&r).await?;
        summary.runs_imported += 1;
    }
    for c in backup.creatures {
        if existing_creature_ids.contains(&c.creature) { continue; }
        db.save_creature(&c).await?;
        summary.creatures_imported += 1;
    }
    for w in backup.weapons {
        if existing_weapon_ids.contains(&w.id) { continue; }
        db.save_weapon(&w).await?;
        summary.weapons_imported += 1;
    }
    for a in backup.amps {
        if existing_amp_ids.contains(&a.id) { continue; }
        db.save_amp(&a).await?;
        summary.amps_imported += 1;
    }
    for a in backup.armor_profiles {
        if existing_armor_ids.contains(&a.id) { continue; }
        db.save_armor(&a).await?;
        summary.armors_imported += 1;
    }
    for f in backup.fap_profiles {
        if existing_fap_ids.contains(&f.id) { continue; }
        db.save_fap(&f).await?;
        summary.faps_imported += 1;
    }
    for l in backup.loadouts {
        if existing_loadout_ids.contains(&l.id) { continue; }
        db.save_loadout(&l).await?;
        summary.loadouts_imported += 1;
    }
    for b in backup.blueprints {
        if existing_blueprint_ids.contains(&b.id) { continue; }
        db.save_blueprint(&b).await?;
        summary.blueprints_imported += 1;
    }

    // MU-Map + player_name
    db.set_setting("player_name", &backup.player_name).await?;
    if let Ok(mu_json) = serde_json::to_string(&backup.mu_map) {
        db.set_setting("mu_map", &mu_json).await?;
    }

    // v4: neue Stores
    for m in backup.materials {
        db.save_material(&m).await?;
        summary.materials_imported += 1;
    }
    for lic in backup.loot_item_configs {
        db.save_loot_item_config(&lic).await?;
    }
    for inv in backup.inventory {
        db.save_inventory_entry(&inv).await?;
    }
    for s in backup.sales {
        db.save_sale(&s).await?;
        summary.sales_imported += 1;
    }
    for p in backup.purchases {
        db.save_purchase(&p).await?;
        summary.purchases_imported += 1;
    }

    Ok(summary)
}
