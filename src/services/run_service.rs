use std::collections::HashMap;
use leptos::prelude::{RwSignal, Set, GetUntracked};
use wasm_bindgen::JsValue;
use js_sys::Reflect;

use crate::domain::types::{
    ArmorProfile, FapProfile, RunConfig, SavedRun, Stats,
};
use crate::persistence::idb::Db;
use crate::services::fs_access::getFileFromHandle;

// ─── RunState ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RunState {
    Idle,
    Configured(RunConfig),
    Running  { config: RunConfig, started_at: u64 },
    Paused   { config: RunConfig, started_at: u64 },
    Stopped  { config: RunConfig, started_at: u64, stopped_at: u64 },
    Saved,
}

// ─── RunService ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RunService {
    pub state:  RwSignal<RunState>,
    pub stats:  RwSignal<Stats>,
    pub offset: RwSignal<u64>,
    pub pec_per_shot: RwSignal<f64>,
    pub mu_map: RwSignal<HashMap<String, f64>>,
}

impl RunService {
    pub fn new() -> Self {
        Self {
            state:  RwSignal::new(RunState::Idle),
            stats:  RwSignal::new(Stats::default()),
            offset: RwSignal::new(0),
            pec_per_shot: RwSignal::new(0.0),
            mu_map: RwSignal::new(HashMap::new()),
        }
    }

    pub fn set_loadout_context(&self, pec_per_shot: f64, mu_map: HashMap<String, f64>) {
        self.pec_per_shot.set(pec_per_shot);
        self.mu_map.set(mu_map);
    }

    /// Setzt die Run-Konfiguration (Idle → Configured).
    pub fn configure(&self, config: RunConfig) {
        self.state.set(RunState::Configured(config));
    }

    /// Startet den Run: Offset = aktuelles Dateiende (nur neue Events), Stats zurücksetzen.
    pub fn start(&self, file_size: u64) {
        let state = self.state.get_untracked();
        if let RunState::Configured(config) = state {
            let now = now_sec();
            self.offset.set(file_size);
            self.stats.set(Stats::default());
            self.state.set(RunState::Running { config, started_at: now });
        }
    }

    /// Pausiert den Run (Running → Paused).
    pub fn pause(&self) {
        let state = self.state.get_untracked();
        if let RunState::Running { config, started_at } = state {
            self.state.set(RunState::Paused { config, started_at });
        }
    }

    /// Setzt den Run fort (Paused → Running).
    pub fn resume(&self) {
        let state = self.state.get_untracked();
        if let RunState::Paused { config, started_at } = state {
            self.state.set(RunState::Running { config, started_at });
        }
    }

    /// BUG-002 Fix: Setzt Offset auf aktuelles Dateiende, ohne Stats zu verändern.
    /// Nur im Zustand Paused verfügbar.
    pub async fn skip_to_now(&self, handle: &JsValue) {
        let size = get_file_size(handle).await;
        self.offset.set(size);
    }

    /// Beendet den Run (Running/Paused → Stopped).
    pub fn stop(&self) {
        let state = self.state.get_untracked();
        let now = now_sec();
        match state {
            RunState::Running { config, started_at } |
            RunState::Paused  { config, started_at } => {
                self.state.set(RunState::Stopped { config, started_at, stopped_at: now });
            }
            _ => {}
        }
    }

    /// Berechnet Kosten und speichert den Run in IDB (Stopped → Saved).
    pub async fn save(
        &self,
        db: &Db,
        armor: Option<&ArmorProfile>,
        fap: Option<&FapProfile>,
        note: Option<String>,
    ) -> Result<SavedRun, JsValue> {
        let state = self.state.get_untracked();
        let stats = self.stats.get_untracked();

        let (config, started_at, stopped_at) = match state {
            RunState::Stopped { config, started_at, stopped_at } => (config, started_at, stopped_at),
            _ => return Err(JsValue::from_str("RunService::save() called in wrong state")),
        };

        let pec_per_shot = self.pec_per_shot.get_untracked();

        // Kostenberechnung
        let ammo_cost_ped   = stats.total_shots as f64 * pec_per_shot / 100.0;
        let armor_decay_ped = armor.map(|a| stats.total_damage_taken * a.repair_pec_per_damage_point / 100.0).unwrap_or(0.0);
        let fap_decay_ped   = fap.map(|f| stats.total_heal_self * f.pec_per_heal_point / 100.0).unwrap_or(0.0);
        let enhancer_cost   = config.enhancer_cost_ped.unwrap_or(0.0);
        let total_cost_ped  = ammo_cost_ped + armor_decay_ped + fap_decay_ped + enhancer_cost;
        let profit_ped      = stats.total_loot_value_ped - total_cost_ped;
        let return_pct_tt   = if total_cost_ped > 0.0 {
            (stats.total_loot_value_ped / total_cost_ped) * 100.0
        } else {
            0.0
        };

        let run = SavedRun {
            id: started_at,
            started_at,
            stopped_at,
            config,
            stats,
            ammo_cost_ped,
            armor_decay_ped,
            fap_decay_ped,
            total_cost_ped,
            profit_ped,
            return_pct_tt,
            note,
        };

        db.save_run(&run).await?;

        // LootItemConfig.droppers aktualisieren: Kreatur als Dropper für jedes geloottete Item eintragen
        let creature_name = run.config.creature.clone();
        let loot_items    = run.stats.loot_items.clone();
        if !creature_name.is_empty() {
            for item_name in loot_items.keys() {
                let existing = db.get_all_loot_item_configs().await.unwrap_or_default();
                let mut cfg = existing.into_iter().find(|c| &c.name == item_name)
                    .unwrap_or_else(|| crate::domain::types::LootItemConfig {
                        name: item_name.clone(), mu_pct: 100.0,
                        droppers: vec![], last_updated_sec: None,
                    });
                if !cfg.droppers.contains(&creature_name) {
                    cfg.droppers.push(creature_name.clone());
                    let _ = db.save_loot_item_config(&cfg).await;
                }
            }
        }

        self.state.set(RunState::Saved);
        Ok(run)
    }

    /// Verwirft den Run und kehrt zu Idle zurück.
    pub fn discard(&self) {
        self.stats.set(Stats::default());
        self.pec_per_shot.set(0.0);
        self.mu_map.set(HashMap::new());
        self.state.set(RunState::Idle);
    }

    /// Startet einen neuen Run (nach Saved oder Stopped → Idle).
    pub fn new_run(&self) {
        self.stats.set(Stats::default());
        self.pec_per_shot.set(0.0);
        self.mu_map.set(HashMap::new());
        self.state.set(RunState::Idle);
    }
}

// ─── Hilfsfunktionen ─────────────────────────────────────────────────────────

/// Aktuelle Unix-Zeit in Sekunden (WASM-kompatibel via js_sys::Date).
pub fn now_sec() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Ermittelt die aktuelle Dateigröße aus einem File-Handle oder Blob.
async fn get_file_size(handle: &JsValue) -> u64 {
    let file_js = if Reflect::has(handle, &JsValue::from_str("getFile")).unwrap_or(false) {
        match getFileFromHandle(handle).await {
            Ok(f) => f,
            Err(_) => return 0,
        }
    } else {
        handle.clone()
    };

    Reflect::get(&file_js, &JsValue::from_str("size"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64
}
