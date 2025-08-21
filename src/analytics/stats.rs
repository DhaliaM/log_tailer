pub(crate) use crate::domain::types::{Event, Stats};
use crate::domain::types::LootItemAgg;

impl Default for Stats {
    fn default() -> Self {
        Self {
            player_hits: 0,
            player_crit_hits: 0,
            player_evades: 0,
            enemy_misses: 0,
            player_misses: 0,
            kills: 0,
            total_loot_value_ped: 0.0,
            total_damage: 0.0,
            loot_items: Default::default(),
        }
    }
}

impl Stats {
    pub fn apply_event(&mut self, ev: &crate::domain::types::Event) {
        use crate::domain::types::Event::*;
        match ev {
            Loot { item, qty: _ , value_ped } => {
                let e = self.loot_items.entry(item.clone())
                    .or_insert(LootItemAgg { total_value_ped: 0.0, event_count: 0 });
                e.total_value_ped += *value_ped as f64;
                e.event_count += 1;
                self.total_loot_value_ped += *value_ped as f64;
            }
            PlayerHit { damage: dmg, critical: crit } => {
                if *crit { self.player_crit_hits += 1; }
                else    { self.player_hits += 1; }
                self.total_damage += dmg;
            }
            EnemyEvaded => self.player_evades += 1,
            EnemyMiss            => self.enemy_misses += 1,
            PlayerMiss           => self.player_misses += 1,
            _ => {}
        }
    }
    
    pub fn record_kill(&mut self) {
        self.kills += 1;
    }
}
