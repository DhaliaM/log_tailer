use std::collections::HashMap;
use serde::{Deserialize, Deserializer};

fn null_as_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
use crate::domain::types::{
    Blueprint, Ingredient, IngredientSource, Material, SubMode, slugify,
};

const NEXUS_BASE: &str = "https://api.entropianexus.com";

// Synthetisches Material für limitierte Waffen/Rüstungen (TT-Lücke → Residue).
// tt_value = 1.0 damit qty == PED Residue direkt.
pub const METAL_RESIDUE_ID:     &str = "metal-residue";
pub const METAL_RESIDUE_NAME:   &str = "Metal Residue";
const METAL_RESIDUE_TT:         f64  = 1.0;
const METAL_RESIDUE_MARKUP:     f64  = 102.0;
const RESIDUE_GAP_THRESHOLD:    f64  = 0.60;

// ─── Bekannte Blueprint-Books ─────────────────────────────────────────────────

/// Hardcodierte populäre Books — Quelle: Nexus API (dedupliziert, Stand 2026-06).
/// Erste Spalte = Anzeigename, zweite = API-Filterwert (URL-encoded wird im Code gemacht).
pub const KNOWN_BOOKS: &[(&str, &str)] = &[
    ("Arkadia Components",     "Arkadia Components"),
    ("Arkadia Weapons",        "Arkadia Weapons"),
    ("Arkadia Armor",          "Arkadia Armor"),
    ("Arkadia Tools",          "Arkadia Tools"),
    ("Arkadia Limited (C)",    "Arkadia Limited (C)"),
    ("Components (Vol. 1)",    "Components (Vol. 1)"),
    ("Components (Vol. 2)",    "Components (Vol. 2)"),
    ("Components (Vol. 3)",    "Components (Vol. 3)"),
    ("NI Components",          "NI Components"),
    ("Weapons (Vol. 1)",       "Weapons (Vol. 1)"),
    ("Weapons (Vol. 2)",       "Weapons (Vol. 2)"),
    ("Armor (Vol. 1)",         "Armor (Vol. 1)"),
    ("Armor (Vol. 2)",         "Armor (Vol. 2)"),
    ("Limited (Vol. 1) (C)",   "Limited (Vol. 1) (C)"),
    ("Blueprints: A.R.C.",     "Blueprints: A.R.C."),
    ("Blueprints: Turrelion",  "Blueprints: Turrelion"),
];

// ─── Nexus API DTOs ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NexusBlueprint {
    #[serde(rename = "Id")]         pub id: u64,
    #[serde(rename = "Name", deserialize_with = "null_as_empty")] pub name: String,
    #[serde(rename = "Product")]    pub product: Option<NexusItem>,
    #[serde(rename = "Materials")]  pub materials: Option<Vec<NexusMaterialEntry>>,
    #[serde(rename = "Properties")] pub properties: NexusBpProps,
    #[serde(rename = "Book")]       pub book: Option<NexusBook>,
}

#[derive(Deserialize)]
pub struct NexusBook {
    #[serde(rename = "Name", deserialize_with = "null_as_empty")] pub name: String,
}

#[derive(Deserialize)]
pub struct NexusBpProps {
    #[serde(rename = "MinimumCraftAmount")] pub min_craft_amount: Option<f64>,
}

#[derive(Deserialize, Clone)]
pub struct NexusItem {
    #[serde(rename = "Name", deserialize_with = "null_as_empty")] pub name: String,
    #[serde(rename = "Properties")] pub properties: NexusItemProps,
}

#[derive(Deserialize, Clone)]
pub struct NexusItemProps {
    #[serde(rename = "Economy")] pub economy: Option<NexusEconomy>,
}

#[derive(Deserialize, Clone)]
pub struct NexusEconomy {
    #[serde(rename = "MaxTT", deserialize_with = "null_as_zero")] pub max_tt: f64,
}

#[derive(Deserialize)]
pub struct NexusMaterialEntry {
    #[serde(rename = "Amount", deserialize_with = "null_as_zero")] pub amount: f64,
    #[serde(rename = "Item")]   pub item: NexusItem,
}

impl NexusItem {
    pub fn tt_value(&self) -> f64 {
        self.properties.economy.as_ref().map(|e| e.max_tt).unwrap_or(0.0)
    }
}

// ─── Fetch ────────────────────────────────────────────────────────────────────

/// Holt alle Blueprints von Nexus und filtert nach dem gewünschten Book.
/// Die Nexus-API ignoriert den ?Book= Parameter serverseitig, daher filtern wir clientseitig.
pub async fn fetch_by_book(book_name: &str) -> Result<Vec<NexusBlueprint>, String> {
    let resp = gloo_net::http::Request::get(&format!("{NEXUS_BASE}/blueprints"))
        .send()
        .await
        .map_err(|e| format!("Netzwerkfehler: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}: Nexus API nicht erreichbar", resp.status()));
    }

    let all: Vec<NexusBlueprint> = resp
        .json()
        .await
        .map_err(|e| format!("JSON-Fehler: {e}"))?;

    let filtered: Vec<NexusBlueprint> = all
        .into_iter()
        .filter(|bp| {
            bp.book.as_ref().map(|b| b.name.as_str()) == Some(book_name)
        })
        .filter(|bp| bp.product.is_some())
        .collect();

    Ok(filtered)
}

// ─── Konvertierung ────────────────────────────────────────────────────────────

pub struct ImportResult {
    pub materials:  Vec<Material>,
    pub blueprints: Vec<Blueprint>,
    pub skipped:    usize,
}

pub fn convert_blueprints(
    nexus_bps: Vec<NexusBlueprint>,
    existing_materials: &[Material],
    existing_blueprints: &[Blueprint],
    now: u64,
) -> ImportResult {
    let existing_mat_markup: HashMap<String, f64> = existing_materials
        .iter().map(|m| (m.id.clone(), m.markup_pct)).collect();
    let existing_bp_markup: HashMap<String, f64> = existing_blueprints
        .iter().map(|b| (b.id.clone(), b.markup_pct)).collect();

    let mut mat_map: HashMap<String, Material> = HashMap::new();
    let mut skipped = 0;

    // Materialien aus allen BP-Zutaten sammeln
    for nbp in &nexus_bps {
        let Some(product) = &nbp.product else { continue };
        let id = slugify(&product.name);
        mat_map.entry(id.clone()).or_insert_with(|| {
            let markup = existing_mat_markup.get(&id).copied().unwrap_or(100.0);
            Material { id, name: product.name.clone(), tt_value: product.tt_value(), markup_pct: markup, group: None }
        });
        for entry in nbp.materials.as_deref().unwrap_or(&[]) {
            let id = slugify(&entry.item.name);
            mat_map.entry(id.clone()).or_insert_with(|| {
                let markup = existing_mat_markup.get(&id).copied().unwrap_or(100.0);
                Material { id, name: entry.item.name.clone(), tt_value: entry.item.tt_value(), markup_pct: markup, group: None }
            });
        }
    }

    // Produkt-ID-Map für Sub-Blueprint-Erkennung
    let product_ids: HashMap<String, String> = nexus_bps.iter()
        .filter_map(|b| b.product.as_ref().map(|p| (p.name.clone(), slugify(&p.name))))
        .collect();

    let mut blueprints = Vec::with_capacity(nexus_bps.len());
    let mut any_residue = false;

    for nbp in &nexus_bps {
        let Some(product) = &nbp.product else { skipped += 1; continue };
        let out_tt = product.tt_value();
        if out_tt <= 0.0 {
            // Kein TT-Wert → überspringen (z.B. Blueprints ohne Economy-Daten)
            skipped += 1;
            continue;
        }

        let bp_id  = slugify(&product.name);
        let markup = existing_bp_markup.get(&bp_id).copied().unwrap_or(100.0);
        let mats   = nbp.materials.as_deref().unwrap_or(&[]);

        let mut ingredients: Vec<Ingredient> = mats.iter().map(|entry| {
            let source = if let Some(sub_id) = product_ids.get(&entry.item.name) {
                IngredientSource::Blueprint { blueprint_id: sub_id.clone() }
            } else {
                IngredientSource::Material { material_id: slugify(&entry.item.name) }
            };
            Ingredient { source, qty: entry.amount, sub_mode: SubMode::Buy, markup_override_pct: None }
        }).collect();

        // Metal Residue für limitierte Waffen/Rüstungen (TT-Lücke > 60%)
        let ingredient_tt: f64 = mats.iter().map(|e| e.amount * e.item.tt_value()).sum();
        let gap = out_tt - ingredient_tt;
        if out_tt > 0.0 && gap / out_tt > RESIDUE_GAP_THRESHOLD && gap > 0.001 {
            ingredients.push(Ingredient {
                source: IngredientSource::Material { material_id: METAL_RESIDUE_ID.to_string() },
                qty: (gap * 100.0).round() / 100.0,
                sub_mode: SubMode::Buy,
                markup_override_pct: None,
            });
            any_residue = true;
        }

        let book_name = nbp.book.as_ref().map(|b| b.name.clone());
        blueprints.push(Blueprint {
            id: bp_id,
            nexus_id: nbp.id,
            name: nbp.name.clone(),
            product_name: product.name.clone(),
            output_qty: nbp.properties.min_craft_amount.unwrap_or(1.0),
            output_tt_value: out_tt,
            markup_pct: markup,
            ingredients,
            book: book_name,
            created_at_sec: now,
        });
    }

    if any_residue {
        mat_map.entry(METAL_RESIDUE_ID.to_string()).or_insert_with(|| {
            let markup = existing_mat_markup.get(METAL_RESIDUE_ID).copied()
                .unwrap_or(METAL_RESIDUE_MARKUP);
            Material {
                id:         METAL_RESIDUE_ID.to_string(),
                name:       METAL_RESIDUE_NAME.to_string(),
                tt_value:   METAL_RESIDUE_TT,
                markup_pct: markup,
                group:      None,
            }
        });
    }

    let mut materials: Vec<Material> = mat_map.into_values().collect();
    materials.sort_by(|a, b| a.name.cmp(&b.name));

    ImportResult { materials, blueprints, skipped }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bp(name: &str, out_tt: f64, mats: Vec<(&str, f64, f64)>) -> NexusBlueprint {
        NexusBlueprint {
            id: 1,
            name: format!("{name} Blueprint"),
            product: Some(NexusItem {
                name: name.to_string(),
                properties: NexusItemProps {
                    economy: Some(NexusEconomy { max_tt: out_tt }),
                },
            }),
            materials: Some(mats.into_iter().map(|(n, amt, tt)| NexusMaterialEntry {
                amount: amt,
                item: NexusItem {
                    name: n.to_string(),
                    properties: NexusItemProps { economy: Some(NexusEconomy { max_tt: tt }) },
                },
            }).collect()),
            properties: NexusBpProps { min_craft_amount: Some(1.0) },
            book: Some(NexusBook { name: "Arkadia Components".into() }),
        }
    }

    #[test]
    fn component_bp_no_residue() {
        let bps = vec![make_bp("Power Regulator", 5.0, vec![
            ("Banite Ingot", 6.0, 0.24),
            ("Hydrogen Gas", 20.0, 0.04),
            ("Insulators",   6.0, 0.20),
        ])];
        let r = convert_blueprints(bps, &[], &[], 0);
        assert_eq!(r.blueprints[0].ingredients.len(), 3);
        assert!(!r.materials.iter().any(|m| m.id == METAL_RESIDUE_ID));
    }

    #[test]
    fn weapon_bp_adds_residue() {
        let bps = vec![make_bp("Herman CAP-37 (L)", 165.0, vec![
            ("Alferix Ingot",   5.0, 2.85),
            ("Power Regulator", 1.0, 5.00),
            ("Energy Chamber",  1.0, 2.00),
        ])];
        let r = convert_blueprints(bps, &[], &[], 0);
        let bp = &r.blueprints[0];
        assert_eq!(bp.ingredients.len(), 4);
        let res = bp.ingredients.iter().find(|i| {
            matches!(&i.source, IngredientSource::Material { material_id } if material_id == METAL_RESIDUE_ID)
        }).expect("residue missing");
        let expected = 165.0 - (5.0 * 2.85 + 5.0 + 2.0);
        assert!((res.qty - expected).abs() < 0.01);
    }

    #[test]
    fn preserves_user_markup_on_reimport() {
        let bps = vec![make_bp("Test Weapon (L)", 100.0, vec![("Iron", 2.0, 1.0)])];
        let existing = vec![Material {
            id: METAL_RESIDUE_ID.into(), name: METAL_RESIDUE_NAME.into(),
            tt_value: METAL_RESIDUE_TT, markup_pct: 105.0, group: None,
        }];
        let r = convert_blueprints(bps, &existing, &[], 0);
        let mat = r.materials.iter().find(|m| m.id == METAL_RESIDUE_ID).unwrap();
        assert_eq!(mat.markup_pct, 105.0);
    }

    #[test]
    fn bp_without_product_is_skipped() {
        let bp = NexusBlueprint {
            id: 1, name: "Bad BP".into(), product: None,
            materials: None, properties: NexusBpProps { min_craft_amount: None },
            book: None,
        };
        let r = convert_blueprints(vec![bp], &[], &[], 0);
        assert_eq!(r.blueprints.len(), 0);
        assert_eq!(r.skipped, 1);
    }

    #[test]
    fn bp_without_tt_is_skipped() {
        let bp = NexusBlueprint {
            id: 2, name: "No TT BP".into(),
            product: Some(NexusItem {
                name: "No TT Item".into(),
                properties: NexusItemProps { economy: None },
            }),
            materials: Some(vec![]),
            properties: NexusBpProps { min_craft_amount: None },
            book: None,
        };
        let r = convert_blueprints(vec![bp], &[], &[], 0);
        assert_eq!(r.blueprints.len(), 0);
        assert_eq!(r.skipped, 1);
    }
}
