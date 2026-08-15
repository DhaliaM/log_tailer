use crate::domain::types::{Amplifier, Loadout, Maturity, Weapon};

/// Aggregiertes Ergebnis über mehrere Maturities (Range-Optimierung).
#[derive(Debug, Clone)]
pub struct RangeLoadoutScore {
    pub loadout_id:      String,
    pub loadout_name:    String,
    pub avg_damage:      f64,   // Konstant pro Loadout
    pub pec_per_shot:    f64,   // Konstant pro Loadout
    /// Ø Schüsse/Kill über alle Maturities der Range
    pub avg_shots_per_kill: f64,
    /// Ø Overkill-Punkte über alle Maturities
    pub avg_overkill_pts:   f64,
    /// Ø Kosten/Kill über alle Maturities (Ranking-Kriterium)
    pub avg_cost_per_kill:  f64,
    /// Worst-case: teuerster Kill (schwerste Maturity)
    pub max_cost_per_kill:  f64,
    /// Scores der einzelnen Maturities für die Detail-Ansicht
    pub per_maturity: Vec<(String, LoadoutScore)>,
}

/// Aggregiert Loadout-Scores über eine Menge von Maturities (gleichgewichtet).
///
/// Gibt pro Loadout einen `RangeLoadoutScore` zurück, der den Durchschnitt
/// über alle übergebenen Maturities darstellt.
pub fn score_loadouts_range(
    maturities: &[&Maturity],
    loadouts: &[(Loadout, Weapon, Option<Amplifier>)],
) -> Vec<RangeLoadoutScore> {
    if maturities.is_empty() { return vec![]; }

    loadouts.iter().map(|(loadout, weapon, amp)| {
        let avg_damage   = loadout.avg_damage(weapon, amp.as_ref());
        let pec_per_shot = loadout.total_pec_per_shot(weapon, amp.as_ref());

        let per_maturity: Vec<(String, LoadoutScore)> = maturities.iter().map(|mat| {
            let hp_midpoint   = (mat.hp_min + mat.hp_max) / 2.0;
            let shots_per_kill = if avg_damage > 0.0 {
                (hp_midpoint / avg_damage).ceil() as u64
            } else {
                u64::MAX
            };
            let score = LoadoutScore {
                loadout_id:       loadout.id.clone(),
                loadout_name:     loadout.name.clone(),
                avg_damage,
                pec_per_shot,
                shots_per_kill,
                avg_overkill_pts: shots_per_kill as f64 * avg_damage - hp_midpoint,
                cost_per_kill_ped: shots_per_kill as f64 * pec_per_shot / 100.0,
            };
            (mat.name.clone(), score)
        }).collect();

        let n = per_maturity.len() as f64;
        let avg_shots      = per_maturity.iter().map(|(_, s)| s.shots_per_kill as f64).sum::<f64>() / n;
        let avg_overkill   = per_maturity.iter().map(|(_, s)| s.avg_overkill_pts).sum::<f64>()    / n;
        let avg_cost       = per_maturity.iter().map(|(_, s)| s.cost_per_kill_ped).sum::<f64>()   / n;
        let max_cost       = per_maturity.iter().map(|(_, s)| s.cost_per_kill_ped)
            .fold(f64::NEG_INFINITY, f64::max);

        RangeLoadoutScore {
            loadout_id:          loadout.id.clone(),
            loadout_name:        loadout.name.clone(),
            avg_damage,
            pec_per_shot,
            avg_shots_per_kill:  avg_shots,
            avg_overkill_pts:    avg_overkill,
            avg_cost_per_kill:   avg_cost,
            max_cost_per_kill:   max_cost,
            per_maturity,
        }
    }).collect()
}

/// Ergebnis der Loadout-Optimierung für eine Kreatur-Maturity.
#[derive(Debug, Clone)]
pub struct LoadoutScore {
    pub loadout_id: String,
    pub loadout_name: String,
    pub avg_damage: f64,
    pub pec_per_shot: f64,
    pub shots_per_kill: u64,
    pub avg_overkill_pts: f64,
    pub cost_per_kill_ped: f64,
}

/// Berechnet LoadoutScores für alle übergebenen Loadouts gegen eine Kreatur-Maturity.
///
/// Formel:
/// - `hp_midpoint    = (maturity.hp_min + maturity.hp_max) / 2`
/// - `shots_per_kill = ceil(hp_midpoint / avg_damage)`
/// - `overkill_pts   = shots_per_kill * avg_damage - hp_midpoint`
/// - `cost_per_kill  = shots_per_kill * pec_per_shot / 100`
pub fn score_loadouts(
    maturity: &Maturity,
    loadouts: &[(Loadout, Weapon, Option<Amplifier>)],
) -> Vec<LoadoutScore> {
    let hp_midpoint = (maturity.hp_min + maturity.hp_max) / 2.0;

    loadouts
        .iter()
        .map(|(loadout, weapon, amp)| {
            let avg_damage  = loadout.avg_damage(weapon, amp.as_ref());
            let pec_per_shot = loadout.total_pec_per_shot(weapon, amp.as_ref());

            let shots_per_kill = if avg_damage > 0.0 {
                (hp_midpoint / avg_damage).ceil() as u64
            } else {
                u64::MAX
            };

            let avg_overkill_pts = shots_per_kill as f64 * avg_damage - hp_midpoint;
            let cost_per_kill_ped = shots_per_kill as f64 * pec_per_shot / 100.0;

            LoadoutScore {
                loadout_id: loadout.id.clone(),
                loadout_name: loadout.name.clone(),
                avg_damage,
                pec_per_shot,
                shots_per_kill,
                avg_overkill_pts,
                cost_per_kill_ped,
            }
        })
        .collect()
}
