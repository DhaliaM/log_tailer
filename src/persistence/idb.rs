use std::rc::Rc;
use idb::{Database, DatabaseEvent, Error, Factory, KeyPath, ObjectStoreParams, TransactionMode};
use wasm_bindgen::JsValue;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen as swb;

use crate::domain::types::Stats;

#[derive(Clone)]
pub struct Db(Rc<Database>);

impl Db {
    #[inline]
    pub fn inner(&self) -> &Database { &self.0 }
}

#[derive(Serialize, Deserialize, Default)]
struct StatsRow {
    player_hits: u64,
    player_crit_hits: u64,
    player_evades: u64,
    enemy_misses: u64,
    player_misses: u64,
}

impl From<StatsRow> for Stats {
    fn from(r: StatsRow) -> Self {
        Self {
            player_hits: r.player_hits,
            player_crit_hits: r.player_crit_hits,
            player_evades: r.player_evades,
            enemy_misses: r.enemy_misses,
            player_misses: r.player_misses,
            total_damage: 0.0,
            kills: 0,
            total_loot_value_ped: 0.0,
            loot_items: Default::default(),
        }
    }
}
impl From<Stats> for StatsRow {
    fn from(s: Stats) -> Self {
        Self {
            player_hits: s.player_hits,
            player_crit_hits: s.player_crit_hits,
            player_evades: s.player_evades,
            enemy_misses: s.enemy_misses,
            player_misses: s.player_misses,
        }
    }
}

const STORE_STATS: &str = "stats";
const KEY_CURRENT: &str = "current";

impl Db {
    pub async fn open(name: &str, version: u32) -> Result<Self, Error> {
        let factory = Factory::new()?;
        let mut req = factory.open(name, Some(version))?;

        req.on_upgrade_needed(|evt| {
            let db = evt.database().unwrap();
            if !db.store_names().iter().any(|s| s == STORE_STATS) {
                let mut p = ObjectStoreParams::new();
                // fester Key "key" -> wir speichern immer unter KEY_CURRENT
                p.key_path(Some(KeyPath::new_single("key")));
                db.create_object_store(STORE_STATS, p).unwrap();
            }
        });

        let inner = req.await?;
        Ok(Db(Rc::new(inner)))
    }

    pub async fn get_stats(&self) -> Result<Stats, Error> {
        let tx = self.inner().transaction(&[STORE_STATS], TransactionMode::ReadOnly)?;
        let store = tx.object_store(STORE_STATS).unwrap();

        let v: Option<JsValue> = store.get(JsValue::from_str(KEY_CURRENT))?.await?;
        tx.await?;

        if let Some(val) = v {
            let row: StatsRow = swb::from_value(val).unwrap_or_default();
            Ok(row.into())
        } else {
            Ok(Stats::default())
        }
    }

    pub async fn bump_stats(&self, delta: Stats) -> Result<(), Error> {
        let mut cur = self.get_stats().await?;
        cur.player_hits       += delta.player_hits;
        cur.player_crit_hits  += delta.player_crit_hits;
        cur.player_evades     += delta.player_evades;
        cur.enemy_misses      += delta.enemy_misses;
        cur.player_misses     += delta.player_misses;

        let tx = self.inner().transaction(&[STORE_STATS], TransactionMode::ReadWrite)?;
        let store = tx.object_store(STORE_STATS).unwrap();

        let row: StatsRow = cur.into();
        let js = swb::to_value(&row).unwrap();
        store.add(&js, Option::from(&JsValue::from_str(KEY_CURRENT)))?.await?;

        tx.commit()?.await?;
        Ok(())
    }
}
