use std::rc::Rc;
use idb::{Database, DatabaseEvent, Error as IdbError, Factory, KeyPath, ObjectStoreParams, TransactionMode};
use wasm_bindgen::JsValue;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen as swb;

use crate::domain::types::{
    Amplifier, ArmorProfile, Blueprint, CreatureConfig, FapProfile, HuntPlan,
    InventoryEntry, Loadout, LootItemConfig, Material, PurchaseRecord, SaleRecord,
    SavedRun, Stats, Weapon,
};

// ─── DB-Konstanten ────────────────────────────────────────────────────────────

const DB_NAME: &str = "log_tailer";
pub const DB_VERSION: u32 = 5;

const STORE_STATS:         &str = "stats";
const STORE_RUNS:          &str = "runs";
const STORE_CREATURES:     &str = "creature_configs";
const STORE_WEAPONS:       &str = "weapons";
const STORE_AMPS:          &str = "amps";
const STORE_ARMORS:        &str = "armor_profiles";
const STORE_FAPS:          &str = "fap_profiles";
const STORE_LOADOUTS:      &str = "loadouts";
const STORE_BLUEPRINTS:    &str = "blueprints";
const STORE_SETTINGS:      &str = "settings";
const STORE_LOOT_ITEMS:    &str = "loot_item_configs";
const STORE_MATERIALS:     &str = "materials";
const STORE_INVENTORY:     &str = "inventory";
const STORE_CRAFTING_SALES:&str = "crafting_sales";
const STORE_STOCK:         &str = "stock";

const KEY_CURRENT: &str = "current";

// ─── DB-Wrapper ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Db(Rc<Database>);

// SAFETY: wasm32 is single-threaded (no atomics), so Db is safe to mark Send.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Db {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for Db {}

impl Db {
    #[inline]
    pub fn inner(&self) -> &Database { &self.0 }

    pub async fn open() -> Result<Self, IdbError> {
        let factory = Factory::new()?;
        let mut req = factory.open(DB_NAME, Some(DB_VERSION))?;

        req.on_upgrade_needed(|evt| {
            let db = evt.database().unwrap();
            let old_version = evt.old_version().unwrap_or(0);

            // v1: stats-Store
            if !db.store_names().iter().any(|s| s == STORE_STATS) {
                let mut p = ObjectStoreParams::new();
                p.key_path(Some(KeyPath::new_single("key")));
                db.create_object_store(STORE_STATS, p).unwrap();
            }

            // v2: alle neuen Stores
            if old_version < 2 {
                // runs: keyPath = "id" (u64 timestamp)
                if !db.store_names().iter().any(|s| s == STORE_RUNS) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("id")));
                    db.create_object_store(STORE_RUNS, p).unwrap();
                }
                // creature_configs: keyPath = "creature"
                if !db.store_names().iter().any(|s| s == STORE_CREATURES) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("creature")));
                    db.create_object_store(STORE_CREATURES, p).unwrap();
                }
                // weapons / amps / armor_profiles / fap_profiles / loadouts / blueprints: keyPath = "id"
                for store in [STORE_WEAPONS, STORE_AMPS, STORE_ARMORS, STORE_FAPS, STORE_LOADOUTS, STORE_BLUEPRINTS] {
                    if !db.store_names().iter().any(|s| s == store) {
                        let mut p = ObjectStoreParams::new();
                        p.key_path(Some(KeyPath::new_single("id")));
                        db.create_object_store(store, p).unwrap();
                    }
                }
                // settings: keyPath = "key"
                if !db.store_names().iter().any(|s| s == STORE_SETTINGS) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("key")));
                    db.create_object_store(STORE_SETTINGS, p).unwrap();
                }
            }

            if old_version < 3 {
                if !db.store_names().iter().any(|s| s == STORE_LOOT_ITEMS) {
                    db.create_object_store(STORE_LOOT_ITEMS, ObjectStoreParams::new()).unwrap();
                }
            }

            // v4: Crafting-Erweiterung (materials, inventory, crafting_sales)
            if old_version < 4 {
                if !db.store_names().iter().any(|s| s == STORE_MATERIALS) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("id")));
                    db.create_object_store(STORE_MATERIALS, p).unwrap();
                }
                if !db.store_names().iter().any(|s| s == STORE_INVENTORY) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("item_id")));
                    db.create_object_store(STORE_INVENTORY, p).unwrap();
                }
                if !db.store_names().iter().any(|s| s == STORE_CRAFTING_SALES) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("id")));
                    db.create_object_store(STORE_CRAFTING_SALES, p).unwrap();
                }
                if !db.store_names().iter().any(|s| s == STORE_STOCK) {
                    let mut p = ObjectStoreParams::new();
                    p.key_path(Some(KeyPath::new_single("id")));
                    db.create_object_store(STORE_STOCK, p).unwrap();
                }
            }
        });

        let inner = req.await?;
        Ok(Db(Rc::new(inner)))
    }
}

// ─── Fehler-Hilfsmakro ────────────────────────────────────────────────────────

fn idb_err(e: IdbError) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn ser_err(e: serde_wasm_bindgen::Error) -> JsValue {
    JsValue::from_str(&e.to_string())
}

// ─── Generische Helpers ───────────────────────────────────────────────────────

impl Db {
    /// Speichert einen serialisierbaren Wert in einen Store (put = upsert).
    async fn put<T: Serialize>(&self, store_name: &str, value: &T) -> Result<(), JsValue> {
        let js = swb::to_value(value).map_err(ser_err)?;
        let tx = self.inner()
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .map_err(idb_err)?;
        let store = tx.object_store(store_name).map_err(idb_err)?;
        store.put(&js, None).map_err(idb_err)?.await.map_err(idb_err)?;
        tx.commit().map_err(idb_err)?.await.map_err(idb_err)?;
        Ok(())
    }

    /// Lädt alle Einträge eines Stores.
    async fn get_all<T: for<'de> Deserialize<'de>>(&self, store_name: &str) -> Result<Vec<T>, JsValue> {
        let tx = self.inner()
            .transaction(&[store_name], TransactionMode::ReadOnly)
            .map_err(idb_err)?;
        let store = tx.object_store(store_name).map_err(idb_err)?;
        let all: Vec<JsValue> = store.get_all(None, None).map_err(idb_err)?.await.map_err(idb_err)?;
        tx.await.map_err(idb_err)?;
        all.into_iter()
            .map(|v| swb::from_value(v).map_err(ser_err))
            .collect()
    }

    /// Löscht einen Eintrag per Key.
    async fn delete_by_key(&self, store_name: &str, key: JsValue) -> Result<(), JsValue> {
        let tx = self.inner()
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .map_err(idb_err)?;
        let store = tx.object_store(store_name).map_err(idb_err)?;
        store.delete(key).map_err(idb_err)?.await.map_err(idb_err)?;
        tx.commit().map_err(idb_err)?.await.map_err(idb_err)?;
        Ok(())
    }
}

// ─── Stats (bestehend, angepasst) ────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct StatsEntry {
    key: String,
    #[serde(flatten)]
    stats: Stats,
}

impl Db {
    pub async fn get_stats(&self) -> Result<Stats, IdbError> {
        let tx = self.inner().transaction(&[STORE_STATS], TransactionMode::ReadOnly)?;
        let store = tx.object_store(STORE_STATS).unwrap();
        let v: Option<JsValue> = store.get(JsValue::from_str(KEY_CURRENT))?.await?;
        tx.await?;
        if let Some(val) = v {
            let entry: StatsEntry = swb::from_value(val).unwrap_or_else(|_| StatsEntry {
                key: KEY_CURRENT.into(),
                stats: Stats::default(),
            });
            Ok(entry.stats)
        } else {
            Ok(Stats::default())
        }
    }

    pub async fn save_stats(&self, stats: &Stats) -> Result<(), IdbError> {
        let entry = StatsEntry { key: KEY_CURRENT.into(), stats: stats.clone() };
        let js = swb::to_value(&entry).unwrap();
        let tx = self.inner().transaction(&[STORE_STATS], TransactionMode::ReadWrite)?;
        let store = tx.object_store(STORE_STATS).unwrap();
        store.put(&js, None)?.await?;
        tx.commit()?.await?;
        Ok(())
    }

    pub async fn bump_stats(&self, delta: Stats) -> Result<(), IdbError> {
        let mut cur = self.get_stats().await?;
        cur.player_hits       += delta.player_hits;
        cur.player_crit_hits  += delta.player_crit_hits;
        cur.player_evades     += delta.player_evades;
        cur.enemy_misses      += delta.enemy_misses;
        cur.player_misses     += delta.player_misses;
        cur.total_shots       += delta.total_shots;
        cur.total_damage      += delta.total_damage;
        cur.total_damage_taken += delta.total_damage_taken;
        cur.total_heal_self   += delta.total_heal_self;
        self.save_stats(&cur).await
    }
}

// ─── Runs ────────────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_run(&self, run: &SavedRun) -> Result<(), JsValue> {
        self.put(STORE_RUNS, run).await
    }

    pub async fn get_all_runs(&self) -> Result<Vec<SavedRun>, JsValue> {
        self.get_all(STORE_RUNS).await
    }

    pub async fn delete_run(&self, id: u64) -> Result<(), JsValue> {
        self.delete_by_key(STORE_RUNS, JsValue::from_f64(id as f64)).await
    }
}

// ─── CreatureConfig ───────────────────────────────────────────────────────────

impl Db {
    pub async fn save_creature(&self, c: &CreatureConfig) -> Result<(), JsValue> {
        self.put(STORE_CREATURES, c).await
    }

    pub async fn get_all_creatures(&self) -> Result<Vec<CreatureConfig>, JsValue> {
        self.get_all(STORE_CREATURES).await
    }

    pub async fn delete_creature(&self, name: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_CREATURES, JsValue::from_str(name)).await
    }
}

// ─── Weapons ─────────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_weapon(&self, w: &Weapon) -> Result<(), JsValue> {
        self.put(STORE_WEAPONS, w).await
    }

    pub async fn get_all_weapons(&self) -> Result<Vec<Weapon>, JsValue> {
        self.get_all(STORE_WEAPONS).await
    }

    pub async fn delete_weapon(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_WEAPONS, JsValue::from_str(id)).await
    }
}

// ─── Amplifiers ───────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_amp(&self, a: &Amplifier) -> Result<(), JsValue> {
        self.put(STORE_AMPS, a).await
    }

    pub async fn get_all_amps(&self) -> Result<Vec<Amplifier>, JsValue> {
        self.get_all(STORE_AMPS).await
    }

    pub async fn delete_amp(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_AMPS, JsValue::from_str(id)).await
    }
}

// ─── ArmorProfiles ────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_armor(&self, a: &ArmorProfile) -> Result<(), JsValue> {
        self.put(STORE_ARMORS, a).await
    }

    pub async fn get_all_armors(&self) -> Result<Vec<ArmorProfile>, JsValue> {
        self.get_all(STORE_ARMORS).await
    }

    pub async fn delete_armor(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_ARMORS, JsValue::from_str(id)).await
    }
}

// ─── FapProfiles ─────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_fap(&self, f: &FapProfile) -> Result<(), JsValue> {
        self.put(STORE_FAPS, f).await
    }

    pub async fn get_all_faps(&self) -> Result<Vec<FapProfile>, JsValue> {
        self.get_all(STORE_FAPS).await
    }

    pub async fn delete_fap(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_FAPS, JsValue::from_str(id)).await
    }
}

// ─── Loadouts ────────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_loadout(&self, l: &Loadout) -> Result<(), JsValue> {
        self.put(STORE_LOADOUTS, l).await
    }

    pub async fn get_all_loadouts(&self) -> Result<Vec<Loadout>, JsValue> {
        self.get_all(STORE_LOADOUTS).await
    }

    pub async fn delete_loadout(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_LOADOUTS, JsValue::from_str(id)).await
    }
}

// ─── Blueprints ───────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_blueprint(&self, b: &Blueprint) -> Result<(), JsValue> {
        self.put(STORE_BLUEPRINTS, b).await
    }

    pub async fn get_all_blueprints(&self) -> Result<Vec<Blueprint>, JsValue> {
        self.get_all(STORE_BLUEPRINTS).await
    }

    pub async fn delete_blueprint(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_BLUEPRINTS, JsValue::from_str(id)).await
    }
}

// ─── LootItemConfigs ──────────────────────────────────────────────────────────

impl Db {
    pub async fn save_loot_item_config(&self, item: &LootItemConfig) -> Result<(), JsValue> {
        let js = swb::to_value(item).map_err(ser_err)?;
        let tx = self.inner()
            .transaction(&[STORE_LOOT_ITEMS], TransactionMode::ReadWrite)
            .map_err(idb_err)?;
        let store = tx.object_store(STORE_LOOT_ITEMS).map_err(idb_err)?;
        let key = JsValue::from_str(&item.name);
        store.put(&js, Some(&key)).map_err(idb_err)?.await.map_err(idb_err)?;
        tx.commit().map_err(idb_err)?.await.map_err(idb_err)?;
        Ok(())
    }

    pub async fn get_all_loot_item_configs(&self) -> Result<Vec<LootItemConfig>, JsValue> {
        self.get_all(STORE_LOOT_ITEMS).await
    }

    pub async fn delete_loot_item_config(&self, name: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_LOOT_ITEMS, JsValue::from_str(name)).await
    }
}

// ─── Materials ────────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_material(&self, m: &Material) -> Result<(), JsValue> {
        self.put(STORE_MATERIALS, m).await
    }

    pub async fn save_materials_batch(&self, items: &[Material]) -> Result<(), JsValue> {
        let tx = self.inner()
            .transaction(&[STORE_MATERIALS], TransactionMode::ReadWrite)
            .map_err(idb_err)?;
        let store = tx.object_store(STORE_MATERIALS).map_err(idb_err)?;
        for item in items {
            let js = swb::to_value(item).map_err(ser_err)?;
            store.put(&js, None).map_err(idb_err)?.await.map_err(idb_err)?;
        }
        tx.commit().map_err(idb_err)?.await.map_err(idb_err)?;
        Ok(())
    }

    pub async fn get_all_materials(&self) -> Result<Vec<Material>, JsValue> {
        self.get_all(STORE_MATERIALS).await
    }

    pub async fn delete_material(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_MATERIALS, JsValue::from_str(id)).await
    }
}

// ─── Inventory ────────────────────────────────────────────────────────────────

impl Db {
    pub async fn save_inventory_entry(&self, e: &InventoryEntry) -> Result<(), JsValue> {
        self.put(STORE_INVENTORY, e).await
    }

    pub async fn get_all_inventory(&self) -> Result<Vec<InventoryEntry>, JsValue> {
        self.get_all(STORE_INVENTORY).await
    }

    #[allow(dead_code)]
    pub async fn delete_inventory_entry(&self, item_id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_INVENTORY, JsValue::from_str(item_id)).await
    }
}

// ─── Crafting Sales (SaleRecord + PurchaseRecord) ─────────────────────────────

impl Db {
    pub async fn save_sale(&self, s: &SaleRecord) -> Result<(), JsValue> {
        self.put(STORE_CRAFTING_SALES, s).await
    }

    pub async fn save_purchase(&self, p: &PurchaseRecord) -> Result<(), JsValue> {
        self.put(STORE_CRAFTING_SALES, p).await
    }

    pub async fn get_all_sales(&self) -> Result<Vec<SaleRecord>, JsValue> {
        let all: Vec<serde_json::Value> = {
            let tx = self.inner()
                .transaction(&[STORE_CRAFTING_SALES], TransactionMode::ReadOnly)
                .map_err(idb_err)?;
            let store = tx.object_store(STORE_CRAFTING_SALES).map_err(idb_err)?;
            let raw: Vec<wasm_bindgen::JsValue> = store.get_all(None, None).map_err(idb_err)?.await.map_err(idb_err)?;
            tx.await.map_err(idb_err)?;
            raw.into_iter()
                .filter_map(|v| swb::from_value(v).ok())
                .collect()
        };
        Ok(all.into_iter()
            .filter(|v| v.get("price_ped").is_some())
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    pub async fn get_all_purchases(&self) -> Result<Vec<PurchaseRecord>, JsValue> {
        let all: Vec<serde_json::Value> = {
            let tx = self.inner()
                .transaction(&[STORE_CRAFTING_SALES], TransactionMode::ReadOnly)
                .map_err(idb_err)?;
            let store = tx.object_store(STORE_CRAFTING_SALES).map_err(idb_err)?;
            let raw: Vec<wasm_bindgen::JsValue> = store.get_all(None, None).map_err(idb_err)?.await.map_err(idb_err)?;
            tx.await.map_err(idb_err)?;
            raw.into_iter()
                .filter_map(|v| swb::from_value(v).ok())
                .collect()
        };
        Ok(all.into_iter()
            .filter(|v| v.get("price_ped").is_none())
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    #[allow(dead_code)]
    pub async fn delete_crafting_sale(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_CRAFTING_SALES, JsValue::from_str(id)).await
    }

    // ── Rohstofflager ─────────────────────────────────────────────────────────

    pub async fn save_stock(&self, e: &crate::domain::types::StockEntry) -> Result<(), JsValue> {
        self.put(STORE_STOCK, e).await
    }

    pub async fn get_all_stock(&self) -> Result<Vec<crate::domain::types::StockEntry>, JsValue> {
        self.get_all(STORE_STOCK).await
    }

    pub async fn delete_stock(&self, id: &str) -> Result<(), JsValue> {
        self.delete_by_key(STORE_STOCK, JsValue::from_str(id)).await
    }
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct SettingEntry {
    key: String,
    value: String,
}

impl Db {
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, JsValue> {
        let tx = self.inner()
            .transaction(&[STORE_SETTINGS], TransactionMode::ReadOnly)
            .map_err(idb_err)?;
        let store = tx.object_store(STORE_SETTINGS).map_err(idb_err)?;
        let v: Option<JsValue> = store.get(JsValue::from_str(key))
            .map_err(idb_err)?.await.map_err(idb_err)?;
        tx.await.map_err(idb_err)?;
        Ok(v.and_then(|js| {
            swb::from_value::<SettingEntry>(js).ok().map(|e| e.value)
        }))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), JsValue> {
        let entry = SettingEntry { key: key.to_string(), value: value.to_string() };
        self.put(STORE_SETTINGS, &entry).await
    }
}

// ─── Hunt-Plan ───────────────────────────────────────────────────────────────

const KEY_HUNT_PLAN: &str = "active_hunt_plan";

impl Db {
    pub async fn get_active_hunt_plan(&self) -> Result<Option<HuntPlan>, JsValue> {
        match self.get_setting(KEY_HUNT_PLAN).await? {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| JsValue::from_str(&e.to_string())),
            None => Ok(None),
        }
    }

    pub async fn set_active_hunt_plan(&self, plan: &HuntPlan) -> Result<(), JsValue> {
        let json = serde_json::to_string(plan)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.set_setting(KEY_HUNT_PLAN, &json).await
    }

    pub async fn clear_active_hunt_plan(&self) -> Result<(), JsValue> {
        self.set_setting(KEY_HUNT_PLAN, "").await
    }
}
