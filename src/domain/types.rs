use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ─── Events ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // Bestehende Events
    PlayerHit { damage: f64, critical: bool },
    EnemyEvaded,
    EnemyMiss,
    PlayerEvaded,
    PlayerMiss,
    Loot { item: String, qty: u64, value_ped: f64, timestamp_sec: u64 },
    Ignored,

    // Neue Events (Phase 2)
    SkillGain { skill: String, amount: f64 },
    AttributeGain { attribute: String, amount: f64 },
    HealSelf { amount: f64 },
    DamageTaken { amount: f64 },
    GlobalKill { creature: String, value_ped: f64, player: String },
}

// ─── Stats ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    // Bestehend
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

    // Neu (Phase 1)
    #[serde(default)]
    pub total_shots: u64,           // = player_hits + crit_hits (Alias für Klarheit)
    #[serde(default)]
    pub total_damage_taken: f64,
    #[serde(default)]
    pub total_heal_self: f64,
    #[serde(default)]
    pub skill_gains: HashMap<String, f64>,
    #[serde(default)]
    pub attribute_gains: HashMap<String, f64>,
    #[serde(default)]
    pub globals: Vec<GlobalEvent>,
    #[serde(default)]
    pub kills_by_maturity: HashMap<String, u64>,
    #[serde(default)]
    pub kill_loots: Vec<KillLoot>,
    #[serde(default)]
    pub kill_timestamps: Vec<u64>,
}

impl Stats {
    pub fn player_attacks(&self) -> u64 {
        self.player_hits + self.player_crit_hits + self.player_evades + self.player_misses
    }
}

// ─── Aggregated Loot ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct LootItemAgg {
    pub total_value_ped: f64,
    pub event_count: u64,
    #[serde(default)]
    pub total_qty: u64,  // Gesamtmenge (für Blueprint-Tracking)
}

// ─── Event-Hilfsstructs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEvent {
    pub creature: String,
    pub value_ped: f64,
    pub player: String,
}

/// Aggregierter Loot-Wert eines einzelnen Kills (für Histogramm)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillLoot {
    pub value_ped: f64,
    pub timestamp_sec: u64,
    /// Erkannte Maturity dieses Kills (None wenn Kreatur/Maturity nicht konfiguriert)
    #[serde(default)]
    pub maturity: Option<String>,
}

// ─── Kreatur-Domain ──────────────────────────────────────────────────────────

/// Ein Maturity-Eintrag einer Kreatur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maturity {
    pub name: String,   // z.B. "Young"
    pub hp_min: f64,
    pub hp_max: f64,
}

/// Kreatur-Konfiguration (wird aus creatures_panel gespeichert)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatureConfig {
    pub creature: String,   // Primary Key / Name
    pub maturities: Vec<Maturity>,
}

impl CreatureConfig {
    /// Gibt die Maturity zurück, deren HP-Range [hp_min, hp_max] den Wert enthält.
    pub fn match_maturity(&self, damage: f64) -> Option<&Maturity> {
        self.maturities
            .iter()
            .find(|m| damage >= m.hp_min && damage <= m.hp_max)
    }
}

// ─── Ausrüstungs-Domain ──────────────────────────────────────────────────────

/// Waffe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weapon {
    pub id: String,     // UUID
    pub name: String,
    pub damage_min: f64,
    pub damage_max: f64,
    pub pec_per_shot: f64,
}

/// Amplifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amplifier {
    pub id: String,
    pub name: String,
    pub flat_damage_bonus: f64,
    pub decay_pec_per_shot: f64,
}

/// Rüstung
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorProfile {
    pub id: String,
    pub name: String,
    pub repair_pec_per_damage_point: f64,
}

/// FAP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FapProfile {
    pub id: String,
    pub name: String,
    pub pec_per_heal_point: f64,
}

/// Loadout = Kombination aus Waffe + optionalem Amp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loadout {
    pub id: String,
    pub name: String,
    pub weapon_id: String,
    pub amp_id: Option<String>,
    /// Anzahl Damage Enhancer (0–10). Jeder Enhancer erhöht Waffenschaden und Waffenverbrauch um 10%.
    /// Amp-Werte (flat_damage_bonus, decay_pec_per_shot) sind davon unberührt.
    #[serde(default)]
    pub damage_enhancers: u8,
}

impl Loadout {
    fn enhancer_factor(&self) -> f64 {
        1.0 + self.damage_enhancers as f64 * 0.1
    }

    /// Gesamte PEC/Shot = weapon.pec_per_shot × (1 + enhancers×0.1) + amp.decay_pec_per_shot
    pub fn total_pec_per_shot(&self, weapon: &Weapon, amp: Option<&Amplifier>) -> f64 {
        weapon.pec_per_shot * self.enhancer_factor()
            + amp.map(|a| a.decay_pec_per_shot).unwrap_or(0.0)
    }

    /// Ø Schaden = weapon_avg × (1 + enhancers×0.1) + amp.flat_damage_bonus
    pub fn avg_damage(&self, weapon: &Weapon, amp: Option<&Amplifier>) -> f64 {
        (weapon.damage_min + weapon.damage_max) / 2.0 * self.enhancer_factor()
            + amp.map(|a| a.flat_damage_bonus).unwrap_or(0.0)
    }
}

// ─── Run-Domain ───────────────────────────────────────────────────────────────

/// Run-Konfiguration (wird vor dem Start gesetzt)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub creature: String,
    pub loadout_id: String,
    pub budget_ped: Option<f64>,
    pub budget_warn_pct: Option<f64>,   // z.B. 80 = Warnung bei 80% verbraucht
    pub target_hp_note: Option<String>,
    pub armor_profile_id: Option<String>,
    pub fap_profile_id: Option<String>,
    #[serde(default)]
    pub event_active: bool,
    #[serde(default)]
    pub enhancer_cost_ped: Option<f64>,   // Loot-Enhancer-Kosten pro Run
}

/// Vollständig berechneter, gespeicherter Run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRun {
    pub id: u64,                    // Timestamp als Primary Key
    pub started_at: u64,            // Unix-Sekunden
    pub stopped_at: u64,
    pub config: RunConfig,
    pub stats: Stats,
    pub ammo_cost_ped: f64,
    pub armor_decay_ped: f64,
    pub fap_decay_ped: f64,
    pub total_cost_ped: f64,
    pub profit_ped: f64,
    pub return_pct_tt: f64,
    #[serde(default)]
    pub note: Option<String>,
}

// ─── Craft-Run-Domain ────────────────────────────────────────────────────────

/// Items die immer als Byproduct erscheinen — ihr alleiniges Vorhandensein = Fehlschlag.
pub const CRAFT_RESIDUE_ITEMS: &[&str] = &[
    "Metal Residue",
    "Robot Component Residue",
    "Animal Oil Residue",
    "Cryogenic Residue",
    "Infernal Residue",
    "Cooking Residue",
    "Shrapnel",
];

pub fn is_residue(item: &str) -> bool {
    CRAFT_RESIDUE_ITEMS.iter().any(|r| *r == item)
}

/// Ein einzelner Craft-Versuch (alle Loot-Zeilen derselben Sekunde).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftAttempt {
    pub timestamp_sec:  u64,
    pub success:        bool,
    /// Output-Items (nicht Residue/Shrapnel)
    pub output_items:   Vec<(String, u64, f64)>,  // (name, qty, value_ped)
    pub residue_ped:    f64,   // Metal Residue + sonstige Residue (zurückgewonnener TT-Wert)
    pub shrapnel_ped:   f64,
    pub total_value_ped:f64,
}

/// Aggregierte Statistiken eines Craft-Runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CraftRunStats {
    pub attempts:         u64,
    pub successes:        u64,
    pub failures:         u64,
    pub total_output_ped: f64,
    pub total_residue_ped:f64,
    pub total_shrapnel_ped:f64,
    /// Output-Items: name → (total_qty, total_ped)
    pub output_items:     std::collections::HashMap<String, (u64, f64)>,
}

impl CraftRunStats {
    pub fn success_rate(&self) -> f64 {
        if self.attempts == 0 { 0.0 }
        else { self.successes as f64 / self.attempts as f64 * 100.0 }
    }
    /// Residue + Shrapnel zusammen (was aus Fehlschlägen zurückkommt)
    #[allow(dead_code)]
    pub fn recovery_ped(&self) -> f64 { self.total_residue_ped + self.total_shrapnel_ped }

    pub fn apply(&mut self, attempt: &CraftAttempt) {
        self.attempts += 1;
        if attempt.success { self.successes += 1; } else { self.failures += 1; }
        self.total_output_ped  += attempt.output_items.iter().map(|(_, _, v)| v).sum::<f64>();
        self.total_residue_ped += attempt.residue_ped;
        self.total_shrapnel_ped += attempt.shrapnel_ped;
        for (name, qty, ped) in &attempt.output_items {
            let e = self.output_items.entry(name.clone()).or_insert((0, 0.0));
            e.0 += qty;
            e.1 += ped;
        }
    }
}

// ─── Crafting-Domain ──────────────────────────────────────────────────────────

/// Slugify: "Banite Ingot" → "banite-ingot"
pub fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Rohstoff / Zutat (mit TT-Wert und Markup)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub tt_value: f64,
    pub markup_pct: f64,
    #[serde(default)]
    pub group: Option<String>,
}

impl Material {
    #[allow(dead_code)]
    pub fn new(name: &str, tt_value: f64) -> Self {
        Self { id: slugify(name), name: name.to_string(), tt_value, markup_pct: 100.0, group: None }
    }
    #[allow(dead_code)]
    pub fn market_price(&self) -> f64 { self.tt_value * (self.markup_pct / 100.0) }
}

/// Ingredient-Quelle: Material oder Sub-Blueprint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngredientSource {
    Material  { material_id: String },
    Blueprint { blueprint_id: String },
}

/// Buy = Marktpreis des Sub-BPs, Craft = Kosten rekursiv berechnen
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubMode { #[default] Buy, Craft }

/// Eine Zutat in einem Blueprint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ingredient {
    pub source: IngredientSource,
    pub qty: f64,
    pub sub_mode: SubMode,
    pub markup_override_pct: Option<f64>,
}

/// Blueprint für Crafting (reiches Modell mit Nexus-Unterstützung)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    pub id: String,
    #[serde(default)]
    pub nexus_id: u64,
    pub name: String,
    #[serde(alias = "output_item")]
    pub product_name: String,
    pub output_qty: f64,
    pub output_tt_value: f64,
    #[serde(alias = "output_mu_pct")]
    pub markup_pct: f64,
    pub ingredients: Vec<Ingredient>,
    /// Blueprint-Buch (z.B. "Arkadia Components") — aus Nexus Import
    #[serde(default)]
    pub book: Option<String>,
    /// Zeitpunkt des Imports (Unix-Sekunden)
    #[serde(default)]
    pub created_at_sec: u64,
}

impl Blueprint {
    #[allow(dead_code)]
    pub fn market_price_per_unit(&self) -> f64 {
        self.output_tt_value * (self.markup_pct / 100.0)
    }

    /// Gibt den Anzeigenamen einer Zutat zurück (material_id oder blueprint_id)
    pub fn ingredient_id(ing: &Ingredient) -> &str {
        match &ing.source {
            IngredientSource::Material  { material_id  } => material_id,
            IngredientSource::Blueprint { blueprint_id } => blueprint_id,
        }
    }
}

// ─── Hunt-Plan ───────────────────────────────────────────────────────────────

/// Aktiver Hunt-Plan: gekoppelter Blueprint + Ziele pro Zutat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuntPlan {
    pub blueprint_id:   String,
    pub blueprint_name: String,
    pub craft_cycles:   u64,
    pub needs:          Vec<HuntNeed>,
}

/// Ein Ziel innerhalb eines Hunt-Plans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuntNeed {
    pub item:         String,
    pub total_needed: f64,
    pub already_have: f64,
}

// ─── Inventar & P&L ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub item_id: String,
    pub item_name: String,
    pub qty: f64,
    pub avg_cost_ped: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleRecord {
    pub id: String,
    pub item_id: String,
    pub item_name: String,
    pub qty: f64,
    pub price_ped: f64,
    pub cost_basis_ped: f64,
    pub sold_at: String,   // ISO-Datum als String (kein chrono-Dep nötig)
}

impl SaleRecord {
    pub fn profit(&self) -> f64 { self.price_ped - self.cost_basis_ped }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseRecord {
    pub id: String,
    pub material_id: String,
    pub material_name: String,
    pub qty: f64,
    pub tt_paid: f64,
    pub markup_paid_pct: f64,
    pub total_ped: f64,
    pub purchased_at: String,
}

#[derive(Debug, Default, Clone)]
pub struct PnlSummary {
    pub total_revenue: f64,
    pub total_cost: f64,
    pub total_purchase_spend: f64,
}

impl PnlSummary {
    pub fn profit(&self) -> f64 { self.total_revenue - self.total_cost }
    pub fn roi_pct(&self) -> f64 {
        if self.total_cost > 0.0 { self.profit() / self.total_cost * 100.0 } else { 0.0 }
    }
}

// ─── Rohstofflager ───────────────────────────────────────────────────────────

/// Lagereintrag für ein Rohmaterial
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockEntry {
    pub id:   String,   // slugify(name)
    pub name: String,
    pub qty:  f64,
}

// ─── MU / Drop-Index ─────────────────────────────────────────────────────────

/// Persistierte MU-Konfiguration für ein Loot-Item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootItemConfig {
    pub name: String,
    pub mu_pct: f64,       // 100.0 = TT-Wert, 110.0 = 10% über TT
    #[serde(default)]
    pub droppers: Vec<String>,  // Kreaturen, die dieses Item gedroppt haben
    #[serde(default)]
    pub last_updated_sec: Option<u64>,
}

/// Globale MU-Map: Item-Name → MU%
/// Wird in settings-Store unter Key "mu_map" gespeichert.
pub type MuMap = HashMap<String, f64>;

/// Drop-Statistics für ein Item einer Kreatur
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DropStats {
    pub drop_rate: f64,         // 0.0–1.0 (Anteil der Kills mit diesem Drop)
    pub avg_tt_per_run: f64,    // Ø TT-Wert pro Run
}

/// Drop-Index: Kreatur → (Item → DropStats)
pub type DropIndex = HashMap<String, HashMap<String, DropStats>>;

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_creature() -> CreatureConfig {
        CreatureConfig {
            creature: "Atrox".to_string(),
            maturities: vec![
                Maturity { name: "Young".to_string(),    hp_min: 100.0, hp_max: 200.0 },
                Maturity { name: "Mature".to_string(),   hp_min: 200.1, hp_max: 350.0 },
                Maturity { name: "Old Alpha".to_string(), hp_min: 350.1, hp_max: 500.0 },
            ],
        }
    }

    #[test]
    fn test_match_maturity_young() {
        let cfg = make_creature();
        let m = cfg.match_maturity(150.0).unwrap();
        assert_eq!(m.name, "Young");
    }

    #[test]
    fn test_match_maturity_mature() {
        let cfg = make_creature();
        let m = cfg.match_maturity(300.0).unwrap();
        assert_eq!(m.name, "Mature");
    }

    #[test]
    fn test_match_maturity_old_alpha() {
        let cfg = make_creature();
        let m = cfg.match_maturity(400.0).unwrap();
        assert_eq!(m.name, "Old Alpha");
    }

    #[test]
    fn test_match_maturity_no_match() {
        let cfg = make_creature();
        assert!(cfg.match_maturity(50.0).is_none());
        assert!(cfg.match_maturity(600.0).is_none());
    }

    fn make_weapon() -> Weapon {
        Weapon { id: "w1".into(), name: "LR-40".into(), damage_min: 14.0, damage_max: 40.0, pec_per_shot: 8.50 }
    }
    fn make_amp() -> Amplifier {
        Amplifier { id: "a1".into(), name: "AS-101".into(), flat_damage_bonus: 12.0, decay_pec_per_shot: 3.20 }
    }
    fn make_loadout(enhancers: u8) -> Loadout {
        Loadout { id: "l1".into(), name: "test".into(), weapon_id: "w1".into(), amp_id: None, damage_enhancers: enhancers }
    }

    #[test]
    fn test_loadout_pec_per_shot_no_amp() {
        assert!((make_loadout(0).total_pec_per_shot(&make_weapon(), None) - 8.50).abs() < 0.001);
    }

    #[test]
    fn test_loadout_pec_per_shot_with_amp() {
        let loadout = Loadout { amp_id: Some("a1".into()), ..make_loadout(0) };
        let amp = make_amp();
        // 8.50 + 3.20 = 11.70
        assert!((loadout.total_pec_per_shot(&make_weapon(), Some(&amp)) - 11.70).abs() < 0.001);
        // avg_damage: (14 + 40) / 2 + 12 = 39.0
        assert!((loadout.avg_damage(&make_weapon(), Some(&amp)) - 39.0).abs() < 0.001);
    }

    #[test]
    fn test_enhancer_weapon_pec_scales() {
        let weapon = make_weapon(); // 8.50 pec/shot
        // 5 enhancer → weapon ×1.5 → 12.75
        assert!((make_loadout(5).total_pec_per_shot(&weapon, None) - 12.75).abs() < 0.001);
        // 10 enhancer → weapon ×2.0 → 17.00
        assert!((make_loadout(10).total_pec_per_shot(&weapon, None) - 17.00).abs() < 0.001);
    }

    #[test]
    fn test_enhancer_amp_unaffected() {
        let weapon = make_weapon(); // 8.50 pec/shot
        let amp = make_amp();       // 3.20 decay, flat +12 dmg
        let loadout = Loadout { amp_id: Some("a1".into()), ..make_loadout(5) };
        // weapon: 8.50 × 1.5 = 12.75, amp: 3.20 → total 15.95
        assert!((loadout.total_pec_per_shot(&weapon, Some(&amp)) - 15.95).abs() < 0.001);
        // damage: (14+40)/2 × 1.5 + 12 = 27×1.5 + 12 = 40.5 + 12 = 52.5
        assert!((loadout.avg_damage(&weapon, Some(&amp)) - 52.5).abs() < 0.001);
    }
}

// ─── CraftRunStats Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod craft_tests {
    use super::*;

    fn attempt(success: bool, output: Vec<(&str, u64, f64)>, residue: f64, shrapnel: f64) -> CraftAttempt {
        CraftAttempt {
            timestamp_sec: 0,
            success,
            output_items: output.into_iter().map(|(n,q,p)| (n.to_string(),q,p)).collect(),
            residue_ped: residue,
            shrapnel_ped: shrapnel,
            total_value_ped: residue + shrapnel + 0.0,
        }
    }

    #[test]
    fn residue_items_recognised() {
        assert!(is_residue("Metal Residue"));
        assert!(is_residue("Robot Component Residue"));
        assert!(is_residue("Shrapnel"));
        assert!(is_residue("Animal Oil Residue"));
        assert!(!is_residue("Super Alloy Mountings"));
        assert!(!is_residue("Robot Heat Sinks"));
        assert!(!is_residue("Durable Hood"));
    }

    #[test]
    fn stats_success_rate() {
        let mut s = CraftRunStats::default();
        s.apply(&attempt(true,  vec![("Super Alloy Mountings", 1, 0.70)], 0.44, 0.0098));
        s.apply(&attempt(true,  vec![("Super Alloy Mountings", 1, 0.70)], 0.36, 0.009));
        s.apply(&attempt(false, vec![], 0.22, 0.007));
        assert_eq!(s.attempts, 3);
        assert_eq!(s.successes, 2);
        assert_eq!(s.failures, 1);
        assert!((s.success_rate() - 66.666).abs() < 0.01);
    }

    #[test]
    fn stats_output_items_aggregated() {
        let mut s = CraftRunStats::default();
        s.apply(&attempt(true, vec![("Super Alloy Mountings", 1, 0.70)], 0.44, 0.0098));
        s.apply(&attempt(true, vec![("Super Alloy Mountings", 3, 2.10)], 0.58, 0.005));
        assert_eq!(s.output_items["Super Alloy Mountings"].0, 4); // 1+3 qty
        assert!((s.output_items["Super Alloy Mountings"].1 - 2.80).abs() < 0.001);
    }

    #[test]
    fn stats_recovery() {
        let mut s = CraftRunStats::default();
        s.apply(&attempt(false, vec![], 0.93, 0.0));  // nur Metal Residue
        s.apply(&attempt(false, vec![], 0.0,  0.007)); // nur Shrapnel
        assert!((s.total_residue_ped - 0.93).abs() < 0.001);
        assert!((s.total_shrapnel_ped - 0.007).abs() < 0.0001);
        assert!((s.recovery_ped() - 0.937).abs() < 0.001);
    }
}

#[cfg(test)]
mod craft_tailer_integration {
    use super::*;
    use crate::domain::parser::parse_line;

    /// Simuliert build_attempt + apply aus craft_tailer.rs mit echten Chat-Zeilen.
    fn process_log(lines: &[&str]) -> Vec<CraftAttempt> {
        let mut attempts: Vec<CraftAttempt> = vec![];
        let mut batch_ts:    u64 = 0;
        let mut batch_items: Vec<(String, u64, f64)> = vec![];

        let flush = |ts: u64, items: Vec<(String, u64, f64)>| -> CraftAttempt {
            let mut output = vec![];
            let mut residue = 0.0f64;
            let mut shrapnel = 0.0f64;
            let mut total = 0.0f64;
            for (name, qty, ped) in items {
                total += ped;
                if name == "Shrapnel"           { shrapnel += ped; }
                else if is_residue(&name)       { residue  += ped; }
                else                            { output.push((name, qty, ped)); }
            }
            CraftAttempt { timestamp_sec: ts, success: !output.is_empty(),
                output_items: output, residue_ped: residue,
                shrapnel_ped: shrapnel, total_value_ped: total }
        };

        for line in lines {
            let Some(ev) = parse_line(line) else { continue };
            let Event::Loot { item, qty, value_ped, timestamp_sec } = ev else { continue };
            if batch_items.is_empty() {
                batch_ts = timestamp_sec; batch_items.push((item, qty, value_ped));
            } else if timestamp_sec <= batch_ts + 1 {
                batch_ts = timestamp_sec; batch_items.push((item, qty, value_ped));
            } else {
                let items = std::mem::take(&mut batch_items);
                attempts.push(flush(batch_ts, items));
                batch_ts = timestamp_sec; batch_items.push((item, qty, value_ped));
            }
        }
        if !batch_items.is_empty() {
            let items = std::mem::take(&mut batch_items);
            attempts.push(flush(batch_ts, items));
        }
        attempts
    }

    #[test]
    fn real_crafting_log_grouping() {
        let lines = [
            // Versuch 1: Erfolg (Super Alloy Mountings)
            "2026-01-17 22:49:15 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
            "2026-01-17 22:49:15 [System] [] You received Metal Residue x (44) Value: 0.4400 PED",
            "2026-01-17 22:49:15 [System] [] You received Shrapnel x (98) Value: 0.0098 PED",
            // Versuch 2: Erfolg (Super Alloy Mountings)
            "2026-01-17 22:49:19 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
            "2026-01-17 22:49:19 [System] [] You received Metal Residue x (36) Value: 0.3600 PED",
            "2026-01-17 22:49:19 [System] [] You received Shrapnel x (90) Value: 0.0090 PED",
            // Versuch 3: Erfolg (Robot Heat Sinks + Durable Hood)
            "2026-01-17 22:49:23 [System] [] You received Robot Heat Sinks x (4) Value: 0.3200 PED",
            "2026-01-17 22:49:23 [System] [] You received Durable Hood x (12) Value: 0.4992 PED",
            "2026-01-17 22:49:23 [System] [] You received Metal Residue x (12) Value: 0.1200 PED",
            "2026-01-17 22:49:23 [System] [] You received Shrapnel x (71) Value: 0.0071 PED",
            // Versuch 4: Fehlschlag (nur Robot Component Residue + Shrapnel)
            "2026-01-17 22:51:55 [System] [] You received Robot Component Residue x (22) Value: 0.2200 PED",
            "2026-01-17 22:51:55 [System] [] You received Shrapnel x (72) Value: 0.0072 PED",
            // Versuch 5: Fehlschlag (nur Metal Residue)
            "2026-01-17 22:52:36 [System] [] You received Metal Residue x (93) Value: 0.9300 PED",
        ];

        let attempts = process_log(&lines);
        assert_eq!(attempts.len(), 5, "5 Craft-Versuche erwartet, got {}", attempts.len());

        // Versuch 1: Erfolg
        assert!(attempts[0].success);
        assert_eq!(attempts[0].output_items.len(), 1);
        assert_eq!(attempts[0].output_items[0].0, "Super Alloy Mountings");
        assert!((attempts[0].residue_ped   - 0.44).abs()  < 0.001);
        assert!((attempts[0].shrapnel_ped  - 0.0098).abs() < 0.0001);

        // Versuch 2: Erfolg
        assert!(attempts[1].success);

        // Versuch 3: Erfolg mit 2 Output-Items
        assert!(attempts[2].success);
        assert_eq!(attempts[2].output_items.len(), 2);
        let names: Vec<&str> = attempts[2].output_items.iter().map(|(n,_,_)| n.as_str()).collect();
        assert!(names.contains(&"Robot Heat Sinks"));
        assert!(names.contains(&"Durable Hood"));

        // Versuch 4: Fehlschlag
        assert!(!attempts[3].success);
        assert!(attempts[3].output_items.is_empty());
        assert!((attempts[3].residue_ped  - 0.22).abs() < 0.001);
        assert!((attempts[3].shrapnel_ped - 0.0072).abs() < 0.0001);

        // Versuch 5: Fehlschlag (nur Metal Residue, kein Shrapnel)
        assert!(!attempts[4].success);
        assert!((attempts[4].residue_ped  - 0.93).abs() < 0.001);
        assert!((attempts[4].shrapnel_ped).abs() < 0.0001);
    }

    #[test]
    fn real_crafting_stats_summary() {
        let lines = [
            "2026-01-17 22:49:15 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
            "2026-01-17 22:49:15 [System] [] You received Metal Residue x (44) Value: 0.4400 PED",
            "2026-01-17 22:49:15 [System] [] You received Shrapnel x (98) Value: 0.0098 PED",
            "2026-01-17 22:49:19 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
            "2026-01-17 22:49:19 [System] [] You received Metal Residue x (36) Value: 0.3600 PED",
            "2026-01-17 22:49:19 [System] [] You received Shrapnel x (90) Value: 0.0090 PED",
            "2026-01-17 22:51:55 [System] [] You received Robot Component Residue x (22) Value: 0.2200 PED",
            "2026-01-17 22:51:55 [System] [] You received Shrapnel x (72) Value: 0.0072 PED",
        ];

        let attempts = process_log(&lines);
        let mut stats = CraftRunStats::default();
        for a in &attempts { stats.apply(a); }

        assert_eq!(stats.attempts, 3);
        assert_eq!(stats.successes, 2);
        assert_eq!(stats.failures,  1);
        assert!((stats.success_rate() - 66.666).abs() < 0.01);

        // Output: 2× Super Alloy Mountings à 0.70 PED = 1.40 PED
        assert!((stats.total_output_ped - 1.40).abs() < 0.001);
        assert_eq!(stats.output_items["Super Alloy Mountings"].0, 2);

        // Residue: 0.44 + 0.36 + 0.22 = 1.02 PED
        assert!((stats.total_residue_ped - 1.02).abs() < 0.001);
    }

    #[test]
    fn non_crafting_lines_ignored() {
        let lines = [
            "2026-01-17 22:49:19 [System] [] Your blueprint Quality Rating has improved",
            "2026-01-17 22:49:24 [Globals] [] Phill No1 Jackson killed a creature (Badger_710R MK2) with a value of 79 PED at Arkadia Underground!",
            "2026-01-17 22:49:25 [Society] [Bellerophon] synchronization",
            "2026-01-17 22:49:15 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
        ];
        let attempts = process_log(&lines);
        // Nur eine Craft-Gruppe (der eine Loot-Event)
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0].success);
    }
}
