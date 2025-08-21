use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub player_hits: u64,
    pub player_crit_hits: u64,
    pub player_evades: u64,
    pub enemy_misses: u64,
    pub player_misses: u64,

    pub kills: u64,
    pub total_loot_value_ped: f64,
    pub total_damage: f64,

    #[serde(default)]
    pub loot_items: HashMap<String, LootItemAgg>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[derive(Default)]
pub struct LootItemAgg {
    pub total_value_ped: f64,
    pub event_count: u64,    
}

impl Stats {
    pub fn player_attacks(&self) -> u64 {
        self.player_hits + self.player_crit_hits + self.player_evades + self.player_misses
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    PlayerHit { damage: f64, critical: bool },
    EnemyEvaded,
    EnemyMiss,
    PlayerEvaded,
    PlayerMiss,
    Loot { item: String, qty: u32, value_ped: f32 },
    Ignored,
}