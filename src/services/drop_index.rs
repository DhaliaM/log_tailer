use std::collections::HashMap;
use crate::domain::types::{DropIndex, DropStats, SavedRun};

/// Gibt alle Kreaturen zurück, die `item_name` gedroppt haben, sortiert nach Drop-Rate (absteigend).
pub fn best_sources_for(item_name: &str, drop_idx: &DropIndex) -> Vec<(String, DropStats)> {
    let mut sources: Vec<(String, DropStats)> = drop_idx
        .iter()
        .filter_map(|(creature, items)| {
            items.get(item_name).map(|stats| (creature.clone(), *stats))
        })
        .collect();
    sources.sort_by(|a, b| b.1.drop_rate.partial_cmp(&a.1.drop_rate).unwrap_or(std::cmp::Ordering::Equal));
    sources
}

/// Baut den Drop-Index aus einer Liste abgeschlossener Runs.
///
/// Für jede Kreatur und jedes geloottete Item wird berechnet:
/// - `drop_rate`: Anteil der Kills, bei denen dieses Item gedroppt wurde
/// - `avg_tt_per_run`: Ø TT-Wert dieses Items pro Run
pub fn build_drop_index(runs: &[SavedRun]) -> DropIndex {
    // creature → item → (kills_with_drop, total_tt, total_kills_in_creature_runs, run_count)
    struct ItemAccum {
        kills_with_drop: u64,   // Kills, bei denen das Item gedropt wurde (approximiert via event_count)
        total_tt: f64,
        run_count: u64,
    }

    // creature → item → accumulator
    let mut accum: HashMap<String, HashMap<String, ItemAccum>> = HashMap::new();
    // creature → total kills across all runs
    let mut creature_kills: HashMap<String, u64> = HashMap::new();
    let mut creature_runs:  HashMap<String, u64> = HashMap::new();

    for run in runs {
        let creature = &run.config.creature;
        *creature_kills.entry(creature.clone()).or_insert(0) += run.stats.kills;
        *creature_runs.entry(creature.clone()).or_insert(0)  += 1;

        let creature_acc = accum.entry(creature.clone()).or_default();
        for (item, loot_agg) in &run.stats.loot_items {
            let entry = creature_acc.entry(item.clone()).or_insert(ItemAccum {
                kills_with_drop: 0,
                total_tt: 0.0,
                run_count: 0,
            });
            // event_count als Proxy für "Anzahl Kills mit diesem Drop"
            entry.kills_with_drop += loot_agg.event_count;
            entry.total_tt        += loot_agg.total_value_ped;
            entry.run_count       += 1;
        }
    }

    // Index zusammenbauen
    let mut index: DropIndex = HashMap::new();
    for (creature, items) in accum {
        let total_kills = *creature_kills.get(&creature).unwrap_or(&1);
        let total_runs  = *creature_runs.get(&creature).unwrap_or(&1);

        let mut item_map = HashMap::new();
        for (item, acc) in items {
            let drop_rate     = acc.kills_with_drop as f64 / total_kills.max(1) as f64;
            let avg_tt_per_run = acc.total_tt / total_runs.max(1) as f64;
            item_map.insert(item, DropStats { drop_rate, avg_tt_per_run });
        }
        index.insert(creature, item_map);
    }

    index
}
