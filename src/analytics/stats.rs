use crate::domain::types::{CreatureConfig, Event, KillLoot, LootItemAgg, Stats};

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
            total_shots: 0,
            total_damage_taken: 0.0,
            total_heal_self: 0.0,
            skill_gains: Default::default(),
            attribute_gains: Default::default(),
            globals: Default::default(),
            kills_by_maturity: Default::default(),
            kill_loots: Default::default(),
            kill_timestamps: Default::default(),
        }
    }
}

impl Stats {
    pub fn apply_event(&mut self, ev: &Event) {
        use Event::*;
        match ev {
            Loot { item, qty, value_ped, .. } => {
                let e = self.loot_items.entry(item.clone())
                    .or_insert(LootItemAgg { total_value_ped: 0.0, event_count: 0, total_qty: 0 });
                e.total_value_ped += value_ped;
                e.event_count     += 1;
                e.total_qty       += qty;
                self.total_loot_value_ped += value_ped;
            }
            PlayerHit { damage, critical } => {
                if *critical {
                    self.player_crit_hits += 1;
                } else {
                    self.player_hits += 1;
                }
                self.total_shots += 1;
                self.total_damage += damage;
            }
            EnemyEvaded => { self.player_evades += 1; self.total_shots += 1; }
            EnemyMiss   => self.enemy_misses += 1,
            PlayerMiss  => { self.player_misses += 1; self.total_shots += 1; }
            PlayerEvaded => {}

            HealSelf { amount } => {
                self.total_heal_self += amount;
            }
            DamageTaken { amount } => {
                self.total_damage_taken += amount;
            }
            SkillGain { skill, amount } => {
                *self.skill_gains.entry(skill.clone()).or_insert(0.0) += amount;
            }
            AttributeGain { attribute, amount } => {
                *self.attribute_gains.entry(attribute.clone()).or_insert(0.0) += amount;
            }
            GlobalKill { creature, value_ped, player } => {
                self.globals.push(crate::domain::types::GlobalEvent {
                    creature: creature.clone(),
                    value_ped: *value_ped,
                    player: player.clone(),
                });
            }
            Ignored => {}
        }
    }

    /// Registriert einen Kill. Aktualisiert maturity-Tracking, kill_loots und kill_timestamps.
    pub fn record_kill(
        &mut self,
        kill_loot_value: f64,
        timestamp_sec: u64,
        creature_config: Option<&CreatureConfig>,
        kill_damage: f64,
    ) {
        self.kills += 1;
        self.kill_timestamps.push(timestamp_sec);

        // Maturity bestimmen, kills_by_maturity updaten und im KillLoot speichern
        let maturity_name: Option<String> = creature_config
            .and_then(|cfg| cfg.match_maturity(kill_damage))
            .map(|mat| {
                *self.kills_by_maturity.entry(mat.name.clone()).or_insert(0) += 1;
                mat.name.clone()
            });

        self.kill_loots.push(KillLoot {
            value_ped: kill_loot_value,
            timestamp_sec,
            maturity: maturity_name,
        });
    }
}
