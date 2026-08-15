use std::collections::HashMap;
use super::types::{Blueprint, IngredientSource, Material, SubMode};

// ─── Parameter & Ergebnistypen ───────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CalcParams {
    pub success_rate: f64,
    pub desired_margin_pct: f64,
    pub markup_overrides: HashMap<String, f64>,
    pub sub_mode_overrides: HashMap<String, SubMode>,
}

impl Default for CalcParams {
    fn default() -> Self {
        Self {
            success_rate: 0.95,
            desired_margin_pct: 0.0,
            markup_overrides: HashMap::new(),
            sub_mode_overrides: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IngredientLine {
    pub label: String,
    pub qty: f64,
    pub unit_cost: f64,
    pub total_cost: f64,
    pub is_sub_blueprint: bool,
    pub sub_mode: SubMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalcResult {
    pub bp_id: String,
    pub bp_name: String,
    pub product_name: String,
    pub output_tt_value: f64,
    pub cost_per_output: f64,
    pub breakeven: f64,
    pub profit_price: f64,
    pub breakdown: Vec<IngredientLine>,
}

// ─── Kern-Algorithmus ────────────────────────────────────────────────────────

pub fn calc_all(
    blueprints: &[Blueprint],
    materials: &[Material],
    params: &CalcParams,
) -> Vec<CalcResult> {
    let bp_map: HashMap<String, &Blueprint> = blueprints.iter().map(|b| (b.id.clone(), b)).collect();
    let mat_map: HashMap<String, &Material> = materials.iter().map(|m| (m.id.clone(), m)).collect();
    blueprints.iter().map(|bp| {
        let (cost, breakdown) = cost_with_breakdown(bp, &bp_map, &mat_map, params, 0);
        build_result(bp, cost, breakdown, params)
    }).collect()
}

pub fn calc_one(
    bp: &Blueprint,
    blueprints: &[Blueprint],
    materials: &[Material],
    params: &CalcParams,
) -> CalcResult {
    let bp_map: HashMap<String, &Blueprint> = blueprints.iter().map(|b| (b.id.clone(), b)).collect();
    let mat_map: HashMap<String, &Material> = materials.iter().map(|m| (m.id.clone(), m)).collect();
    let (cost, breakdown) = cost_with_breakdown(bp, &bp_map, &mat_map, params, 0);
    build_result(bp, cost, breakdown, params)
}

fn build_result(bp: &Blueprint, cost: f64, breakdown: Vec<IngredientLine>, params: &CalcParams) -> CalcResult {
    CalcResult {
        bp_id: bp.id.clone(),
        bp_name: bp.name.clone(),
        product_name: bp.product_name.clone(),
        output_tt_value: bp.output_tt_value,
        cost_per_output: cost,
        breakeven: cost,
        profit_price: cost * (1.0 + params.desired_margin_pct / 100.0),
        breakdown,
    }
}

fn cost_with_breakdown(
    bp: &Blueprint,
    bp_map: &HashMap<String, &Blueprint>,
    mat_map: &HashMap<String, &Material>,
    params: &CalcParams,
    depth: u8,
) -> (f64, Vec<IngredientLine>) {
    if depth > 20 { return (0.0, vec![]); }

    let mut total = 0.0;
    let mut lines = Vec::new();

    for ing in &bp.ingredients {
        match &ing.source {
            IngredientSource::Material { material_id } => {
                let Some(mat) = mat_map.get(material_id.as_str()) else { continue };
                let markup = params.markup_overrides.get(material_id)
                    .copied()
                    .or(ing.markup_override_pct)
                    .unwrap_or(mat.markup_pct);
                let unit_cost  = mat.tt_value * (markup / 100.0);
                let line_cost  = ing.qty * unit_cost;
                total += line_cost;
                lines.push(IngredientLine {
                    label: mat.name.clone(),
                    qty: ing.qty,
                    unit_cost,
                    total_cost: line_cost,
                    is_sub_blueprint: false,
                    sub_mode: SubMode::Buy,
                });
            }
            IngredientSource::Blueprint { blueprint_id } => {
                let Some(sub_bp) = bp_map.get(blueprint_id.as_str()) else { continue };
                let mode = params.sub_mode_overrides.get(blueprint_id)
                    .cloned()
                    .unwrap_or_else(|| ing.sub_mode.clone());

                let unit_cost = match &mode {
                    SubMode::Buy => {
                        let markup = params.markup_overrides.get(blueprint_id)
                            .copied()
                            .or(ing.markup_override_pct)
                            .unwrap_or(sub_bp.markup_pct);
                        sub_bp.output_tt_value * (markup / 100.0)
                    }
                    SubMode::Craft => {
                        let (sub_cost, _) = cost_with_breakdown(sub_bp, bp_map, mat_map, params, depth + 1);
                        sub_cost
                    }
                };
                let line_cost = ing.qty * unit_cost;
                total += line_cost;
                lines.push(IngredientLine {
                    label: sub_bp.product_name.clone(),
                    qty: ing.qty,
                    unit_cost,
                    total_cost: line_cost,
                    is_sub_blueprint: true,
                    sub_mode: mode,
                });
            }
        }
    }

    let cost_per_unit = (total / params.success_rate) / bp.output_qty.max(1.0);
    (cost_per_unit, lines)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::{Blueprint, Ingredient, IngredientSource, Material, SubMode};

    fn mat(id: &str, name: &str, tt: f64, mu: f64) -> Material {
        Material { id: id.into(), name: name.into(), tt_value: tt, markup_pct: mu, group: None }
    }

    fn gas_plug_bp() -> Blueprint {
        Blueprint {
            id: "gas-plug".into(), nexus_id: 1145,
            name: "Gas Plug Blueprint".into(), product_name: "Gas Plug".into(),
            output_qty: 1.0, output_tt_value: 1.5, markup_pct: 100.0,
            book: None, created_at_sec: 0,
            ingredients: vec![
                Ingredient { source: IngredientSource::Material { material_id: "lysterium-ingot".into() }, qty: 8.0,  sub_mode: SubMode::Buy, markup_override_pct: None },
                Ingredient { source: IngredientSource::Material { material_id: "oil".into()            }, qty: 28.0, sub_mode: SubMode::Buy, markup_override_pct: None },
                Ingredient { source: IngredientSource::Material { material_id: "veda-ingot".into()     }, qty: 5.0,  sub_mode: SubMode::Buy, markup_override_pct: None },
            ],
        }
    }

    fn test_materials() -> Vec<Material> {
        vec![
            mat("lysterium-ingot", "Lysterium Ingot", 0.03, 115.0),
            mat("oil",             "Oil",             0.02, 100.0),
            mat("veda-ingot",      "Veda Ingot",      0.18, 100.0),
        ]
    }

    #[test]
    fn gas_plug_95pct_success() {
        let bp = gas_plug_bp();
        let bps = vec![bp.clone()];
        let mats = test_materials();
        let params = CalcParams { success_rate: 0.95, ..Default::default() };
        let result = calc_one(&bp, &bps, &mats, &params);
        // (8*0.03*1.15 + 28*0.02*1.00 + 5*0.18*1.00) / 0.95 ≈ 1.8274
        let expected = (8.0*0.03*1.15 + 28.0*0.02*1.00 + 5.0*0.18*1.00) / 0.95;
        assert!((result.cost_per_output - expected).abs() < 0.0001,
            "Expected {expected:.4}, got {:.4}", result.cost_per_output);
        assert_eq!(result.breakdown.len(), 3);
    }

    #[test]
    fn gas_plug_90pct_success() {
        let bp = gas_plug_bp();
        let bps = vec![bp.clone()];
        let mats = test_materials();
        let params = CalcParams { success_rate: 0.90, ..Default::default() };
        let result = calc_one(&bp, &bps, &mats, &params);
        let expected = (8.0*0.03*1.15 + 28.0*0.02 + 5.0*0.18) / 0.90;
        assert!((result.cost_per_output - expected).abs() < 0.0001);
    }

    #[test]
    fn profit_price_applies_margin() {
        let bp = gas_plug_bp();
        let bps = vec![bp.clone()];
        let mats = test_materials();
        let params = CalcParams { success_rate: 0.95, desired_margin_pct: 10.0, ..Default::default() };
        let result = calc_one(&bp, &bps, &mats, &params);
        assert!((result.profit_price - result.cost_per_output * 1.10).abs() < 0.0001);
    }

    #[test]
    fn markup_override_applied() {
        let bp = gas_plug_bp();
        let bps = vec![bp.clone()];
        let mats = test_materials();
        let mut overrides = HashMap::new();
        overrides.insert("lysterium-ingot".to_string(), 120.0);
        let params = CalcParams { success_rate: 0.95, markup_overrides: overrides, ..Default::default() };
        let result = calc_one(&bp, &bps, &mats, &params);
        let expected = (8.0*0.03*1.20 + 28.0*0.02 + 5.0*0.18) / 0.95;
        assert!((result.cost_per_output - expected).abs() < 0.0001);
    }

    #[test]
    fn recursive_sub_blueprint_craft_mode() {
        let gas_plug = gas_plug_bp();
        let adv_bp = Blueprint {
            id: "advanced-component".into(), nexus_id: 0,
            name: "Advanced Component BP".into(), product_name: "Advanced Component".into(),
            output_qty: 1.0, output_tt_value: 6.0, markup_pct: 100.0,
            book: None, created_at_sec: 0,
            ingredients: vec![
                Ingredient { source: IngredientSource::Blueprint { blueprint_id: "gas-plug".into() }, qty: 3.0, sub_mode: SubMode::Craft, markup_override_pct: None },
                Ingredient { source: IngredientSource::Material  { material_id: "oil".into()       }, qty: 10.0, sub_mode: SubMode::Buy, markup_override_pct: None },
            ],
        };
        let bps   = vec![gas_plug.clone(), adv_bp.clone()];
        let mats  = test_materials();
        let params = CalcParams { success_rate: 0.95, ..Default::default() };
        let gas_cost = calc_one(&gas_plug, &bps, &mats, &params).cost_per_output;
        let result   = calc_one(&adv_bp,  &bps, &mats, &params);
        let expected = (3.0 * gas_cost + 10.0 * 0.02) / 0.95;
        assert!((result.cost_per_output - expected).abs() < 0.0001,
            "Expected {expected:.4}, got {:.4}", result.cost_per_output);
    }
}
