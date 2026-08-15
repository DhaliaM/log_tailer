# log_tailer – Developer Specification

**Stand:** 2026-03-21
**Stack:** Rust · WASM · Leptos · IndexedDB · PWA
**Basis:** `requirements.md` + Analyse des bestehenden Codes

---

## Inhaltsverzeichnis

1. [Aktueller Stand](#1-aktueller-stand)
2. [Architektur-Übersicht](#2-architektur-übersicht)
3. [Phase 1 – Datenmodelle](#3-phase-1--datenmodelle)
4. [Phase 2 – Parser vervollständigen](#4-phase-2--parser-vervollständigen)
5. [Phase 3 – IndexedDB Stores & CRUD](#5-phase-3--indexeddb-stores--crud)
6. [Phase 4 – Services](#6-phase-4--services)
7. [Phase 5 – Run-Panel (UI)](#7-phase-5--run-panel-ui)
8. [Phase 6 – Creatures-Panel (UI)](#8-phase-6--creatures-panel-ui)
9. [Phase 7 – Loadouts-Panel (UI)](#9-phase-7--loadouts-panel-ui)
10. [Phase 8 – History-Panel (UI)](#10-phase-8--history-panel-ui)
11. [Phase 9 – Analyse-Panel (UI)](#11-phase-9--analyse-panel-ui)
12. [Phase 10 – Crafting-Panel (UI)](#12-phase-10--crafting-panel-ui)
13. [Phase 11 – Einstellungen-Panel (UI)](#13-phase-11--einstellungen-panel-ui)
14. [Bug-Fixes](#14-bug-fixes)
15. [Nicht-funktionale Anforderungen](#15-nicht-funktionale-anforderungen)
16. [Akzeptanzkriterien](#16-akzeptanzkriterien)

---

## 1. Aktueller Stand

### Was bereits läuft

| Komponente | Status |
|---|---|
| Polling-Loop (1 s, Offset-basiert) | ✅ implementiert |
| File System Access API + Fallback | ✅ implementiert |
| Parser: PlayerHit, PlayerHitCrit, EnemyEvaded, EnemyMiss, PlayerEvaded, PlayerMiss, Loot | ✅ implementiert |
| Kill-Erkennung (gleicher Sekunden-Timestamp) | ✅ implementiert |
| IndexedDB: ein `stats`-Store | ✅ implementiert |
| Basis-UI: Datei wählen, Start, Stop, Item-Tabelle mit MU% | ✅ implementiert |
| PWA: Service-Worker-Registrierung | ✅ implementiert |

### Was komplett fehlt

- Domain-Modelle: CreatureConfig, SavedRun, Weapon, Amplifier, Armor, FAP, Loadout, Blueprint
- Parser-Events: SkillGain, AttributeGain, HealSelf, DamageTaken, GlobalKill
- IndexedDB: alle Stores außer `stats`
- 6 von 7 UI-Panels
- Run-Lifecycle-State-Machine (IDLE → CONFIGURED → RUNNING ⟷ PAUSED → STOPPED → SAVED)
- BUG-001 und BUG-002 (s. Abschnitt 14)

---

## 2. Architektur-Übersicht

```
src/
├── domain/
│   ├── mod.rs
│   ├── types.rs        ← alle Domain-Structs (Phase 1)
│   └── parser.rs       ← alle 10 Event-Typen (Phase 2)
├── analytics/
│   ├── mod.rs
│   ├── stats.rs        ← Event-Aggregation (Phase 4)
│   └── optimizer.rs    ← Loadout-Optimizer (Phase 9)
├── persistence/
│   ├── mod.rs
│   ├── idb.rs          ← DB-Wrapper + alle Stores (Phase 3)
│   └── export.rs       ← JSON Export/Import (Phase 11)
├── services/
│   ├── mod.rs
│   ├── fs_access.rs    ← bereits fertig
│   ├── tailer.rs       ← Polling-Loop (Bug-002 Fix in Phase 14)
│   ├── run_service.rs  ← Run-Lifecycle (Phase 4)
│   └── drop_index.rs   ← Drop-Index für Crafting (Phase 10)
└── components/
    ├── mod.rs
    ├── app_shell.rs    ← Tab-Navigation (Phase 5)
    ├── run_panel.rs    ← 🎯 Run (Phase 5)
    ├── creatures_panel.rs  ← 🐾 (Phase 6)
    ├── loadouts_panel.rs   ← 🔫 (Phase 7)
    ├── history_panel.rs    ← 📊 (Phase 8)
    ├── analyse_panel.rs    ← 📈 (Phase 9)
    ├── crafting_panel.rs   ← 🔨 (Phase 10)
    └── settings_panel.rs   ← ⚙️ (Phase 11)
```

**Architektur-Regeln (non-negotiable):**

- Components kennen nur Services, nie direkt IDB oder Parser
- Services kennen nur Domain-Structs und Persistence-Contracts
- Domain-Structs importieren keine Services, kein IDB, keine Web-APIs
- DTOs zwischen Schichten sind reine Datencontainer (keine Methoden)

---

## 3. Phase 1 – Datenmodelle

**Datei:** `src/domain/types.rs`
**Ziel:** Alle Domain-Structs definieren, die für alle anderen Phasen Voraussetzung sind.

### 3.1 Erweiterung: `Event`-Enum

Füge folgende Varianten zum bestehenden `Event`-Enum hinzu:

```rust
pub enum Event {
    // bereits vorhanden:
    PlayerHit { damage: f64, crit: bool },
    EnemyEvaded,
    EnemyMiss,
    PlayerEvaded,
    PlayerMiss,
    Loot { item: String, qty: u64, value_ped: f64, timestamp_sec: u64 },
    Ignored,

    // NEU:
    SkillGain { skill: String, amount: f64 },
    AttributeGain { attribute: String, amount: f64 },
    HealSelf { amount: f64 },
    DamageTaken { amount: f64 },
    GlobalKill { creature: String, value_ped: f64, player: String },
}
```

### 3.2 Erweiterung: `Stats`

Ergänze die bestehende `Stats`-Struct:

```rust
pub struct Stats {
    // bereits vorhanden:
    pub player_hits: u64,
    pub player_crit_hits: u64,
    pub player_evades: u64,
    pub enemy_misses: u64,
    pub player_misses: u64,
    pub kills: u64,
    pub total_loot_value_ped: f64,
    pub total_damage: f64,
    pub loot_items: HashMap<String, LootItemAgg>,

    // NEU:
    pub total_shots: u64,               // = player_hits (alias, für Klarheit)
    pub total_damage_taken: f64,
    pub total_heal_self: f64,
    pub skill_gains: HashMap<String, f64>,
    pub attribute_gains: HashMap<String, f64>,
    pub globals: Vec<GlobalEvent>,
    pub kills_by_maturity: HashMap<String, u64>,
    pub kill_loots: Vec<KillLoot>,      // Loot-Wert pro Kill (für Histogramm)
    pub kill_timestamps: Vec<u64>,      // Unix-Sekunden pro Kill (für Kills/h)
}
```

### 3.3 Neue Structs

```rust
pub struct GlobalEvent {
    pub creature: String,
    pub value_ped: f64,
    pub player: String,
}

/// Aggregierter Loot-Wert eines einzelnen Kills (für Histogramm)
pub struct KillLoot {
    pub value_ped: f64,
    pub timestamp_sec: u64,
}

/// Ein Maturity-Eintrag einer Kreatur
pub struct Maturity {
    pub name: String,       // z.B. "Young"
    pub hp_min: f64,
    pub hp_max: f64,
}

/// Kreatur-Konfiguration (wird aus creatures_panel gespeichert)
pub struct CreatureConfig {
    pub creature: String,   // Primary Key / Name
    pub maturities: Vec<Maturity>,
}

impl CreatureConfig {
    /// Gibt die Maturity zurück, deren HP-Range [hp_min, hp_max] den Wert enthält.
    /// Verwendet hp_midpoint = (hp_min + hp_max) / 2 für Overkill-Berechnung.
    pub fn match_maturity(&self, damage: f64) -> Option<&Maturity> { ... }
}

/// Waffe
pub struct Weapon {
    pub id: String,         // UUID
    pub name: String,
    pub damage_min: f64,
    pub damage_max: f64,
    pub pec_per_shot: f64,
}

/// Amplifier
pub struct Amplifier {
    pub id: String,
    pub name: String,
    pub flat_damage_bonus: f64,
    pub decay_pec_per_shot: f64,
}

/// Rüstung
pub struct ArmorProfile {
    pub id: String,
    pub name: String,
    pub repair_pec_per_damage_point: f64,
}

/// FAP
pub struct FapProfile {
    pub id: String,
    pub name: String,
    pub pec_per_heal_point: f64,
}

/// Loadout = Kombination aus Waffe + optionalem Amp
pub struct Loadout {
    pub id: String,
    pub name: String,
    pub weapon_id: String,
    pub amp_id: Option<String>,
}

impl Loadout {
    /// Gesamte PEC/Shot = weapon.pec_per_shot + amp.decay_pec_per_shot (wenn vorhanden)
    pub fn total_pec_per_shot(&self, weapon: &Weapon, amp: Option<&Amplifier>) -> f64 { ... }

    /// Ø Schaden = (weapon.damage_min + weapon.damage_max) / 2 + amp.flat_damage_bonus
    pub fn avg_damage(&self, weapon: &Weapon, amp: Option<&Amplifier>) -> f64 { ... }
}

/// Run-Konfiguration (wird vor dem Start gesetzt)
pub struct RunConfig {
    pub creature: String,
    pub loadout_id: String,
    pub budget_ped: Option<f64>,
    pub budget_warn_pct: Option<f64>,   // z.B. 80 = Warnung bei 80% verbraucht
    pub target_hp_note: Option<String>,
    pub armor_profile_id: Option<String>,
    pub fap_profile_id: Option<String>,
}

/// Vollständig berechneter, gespeicherter Run
pub struct SavedRun {
    pub id: u64,                        // Timestamp als Primary Key
    pub started_at: u64,                // Unix-Sekunden
    pub stopped_at: u64,
    pub config: RunConfig,
    pub stats: Stats,
    pub ammo_cost_ped: f64,
    pub armor_decay_ped: f64,
    pub fap_decay_ped: f64,
    pub total_cost_ped: f64,
    pub profit_ped: f64,
    pub return_pct_tt: f64,
}

/// Blueprint für Crafting
pub struct Blueprint {
    pub id: String,
    pub name: String,
    pub ingredients: Vec<BlueprintIngredient>,
    pub output_item: String,
    pub output_qty: u64,
    pub output_tt_value: f64,
    pub output_mu_pct: f64,
}

pub struct BlueprintIngredient {
    pub item: String,
    pub qty: u64,
    pub tt_value_per_unit: f64,
    pub mu_pct: f64,
}

/// Globale MU-Map: Item-Name → MU%
/// Wird in settings-Store unter Key "mu_map" gespeichert.
pub type MuMap = HashMap<String, f64>;

/// Drop-Index: Kreatur → (Item → DropStats)
pub struct DropStats {
    pub drop_rate: f64,         // 0.0–1.0 (Anteil der Kills mit diesem Drop)
    pub avg_tt_per_run: f64,    // Ø TT-Wert pro Run
}
pub type DropIndex = HashMap<String, HashMap<String, DropStats>>;
```

**Akzeptanzkriterien Phase 1:**
- [ ] Alle Structs kompilieren ohne Fehler
- [ ] Keine Imports aus Services, IDB oder Web-APIs in `types.rs`
- [ ] `CreatureConfig::match_maturity()` gibt korrekte Maturity zurück (Unit-Test mit 3 Maturities)
- [ ] `Loadout::total_pec_per_shot()` addiert Amp-Decay korrekt (Unit-Test)

---

## 4. Phase 2 – Parser vervollständigen

**Datei:** `src/domain/parser.rs`

### 4.1 Fehlende Regex

Füge folgende Muster zur `Lazy<Regex>`-Sammlung hinzu:

```
SKILL_GAIN:
  r"You have gained ([\d.]+) experience in your (.+?) skill"
  Gruppen: (amount, skill_name)

ATTRIBUTE_GAIN:
  r"You have gained ([\d.]+) (.+)"
  Gruppen: (amount, attribute_name)
  Hinweis: nur matchen wenn SKILL_GAIN nicht greift (Reihenfolge beachten)

HEAL_SELF:
  r"You healed yourself ([\d.]+) points"
  Gruppen: (amount)

DAMAGE_TAKEN:
  r"You took ([\d.]+) points of damage"
  Gruppen: (amount)

GLOBAL_KILL:
  r#"\[Globals\] \[\] (.+?) killed a creature \((.+?)\) with a value of ([\d.]+) PED"#
  Gruppen: (player_name, creature_name, value_ped)
```

### 4.2 `parse_line()` erweitern

Die bestehende Funktion filtert bereits auf `[System] []`. Für `GlobalKill` muss der Filter auf `[Globals] []` erweitert werden:

```rust
pub fn parse_line(line: &str) -> Option<Event> {
    // GlobalKill: aus [Globals]-Zeilen
    if line.contains("[Globals] []") {
        return parse_global_kill(line);
    }

    // Alle anderen Events: nur [System]-Zeilen
    if !line.contains("[System] []") {
        return None;
    }

    // bestehende Matches...
    // + neue Matches für SkillGain, AttributeGain, HealSelf, DamageTaken
}
```

### 4.3 Timestamp-Extraktion

Jede Log-Zeile beginnt mit `YYYY-MM-DD HH:MM:SS`. Extrahiere den Unix-Timestamp als `u64`:

```rust
pub fn extract_timestamp_sec(line: &str) -> Option<u64> {
    // Parse "2026-03-21 19:10:17" → Unix-Sekunden
    // Verwende chrono oder manuelles Parsing
}
```

Diese Funktion wird vom Tailer genutzt für:
1. Kill-Erkennung (gleicher Sekunden-Timestamp)
2. Run-Start/Stop-Zeitstempel

**Akzeptanzkriterien Phase 2:**
- [ ] Unit-Tests für alle 10 Event-Typen mit den Beispiel-Zeilen aus `requirements.md`
- [ ] `parse_line("[Rookie]...")` → `None`
- [ ] `parse_line("[Globals] [] Foo killed ...")` → `Some(GlobalKill { ... })`
- [ ] Timestamp-Extraktion gibt korrekten Unix-Wert zurück
- [ ] Reihenfolge: SkillGain-Regex vor AttributeGain-Regex (Overlap-Problem)

---

## 5. Phase 3 – IndexedDB Stores & CRUD

**Datei:** `src/persistence/idb.rs`

### 5.1 Schema-Version hochsetzen

```rust
const DB_VERSION: u32 = 2; // war 1
```

### 5.2 Alle Stores anlegen (in `on_upgrade_needed`)

```rust
// Neue Stores (zusätzlich zum bestehenden "stats"):
"runs"             → keyPath: "id" (u64 Timestamp)
"creature_configs" → keyPath: "creature" (String)
"weapons"          → keyPath: "id" (UUID String)
"amps"             → keyPath: "id"
"armor_profiles"   → keyPath: "id"
"fap_profiles"     → keyPath: "id"
"loadouts"         → keyPath: "id"
"blueprints"       → keyPath: "id"
"settings"         → keyPath: "key" (String)
```

### 5.3 CRUD-Operationen

Implementiere für jeden Store eine Funktion-Familie. Beispiel für `runs`:

```rust
impl Db {
    pub async fn save_run(&self, run: &SavedRun) -> Result<(), JsValue>;
    pub async fn get_all_runs(&self) -> Result<Vec<SavedRun>, JsValue>;
    pub async fn delete_run(&self, id: u64) -> Result<(), JsValue>;
}
```

Gleiches Muster für:
- `creature_configs`: `save_creature`, `get_all_creatures`, `delete_creature`
- `weapons`: `save_weapon`, `get_all_weapons`, `delete_weapon`
- `amps`: `save_amp`, `get_all_amps`, `delete_amp`
- `armor_profiles`: `save_armor`, `get_all_armors`, `delete_armor`
- `fap_profiles`: `save_fap`, `get_all_faps`, `delete_fap`
- `loadouts`: `save_loadout`, `get_all_loadouts`, `delete_loadout`
- `blueprints`: `save_blueprint`, `get_all_blueprints`, `delete_blueprint`

Für `settings`:

```rust
impl Db {
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, JsValue>;
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), JsValue>;
}
```

Settings-Keys:
- `"player_name"` → String
- `"mu_map"` → JSON-serialisierte `MuMap`
- `"last_creature"` → String
- `"pec_per_shot"` → String (f64)

### 5.4 Serialisierung

Alle Structs müssen `serde::Serialize + serde::Deserialize` implementieren.
Serialisierung via `serde_wasm_bindgen` (bereits im Projekt, prüfen ob vorhanden, sonst `Cargo.toml` ergänzen).

### 5.5 Migration

```rust
fn on_upgrade_needed(db: &IdbDatabase, old_version: u32) {
    if old_version < 2 {
        // alle neuen Stores anlegen
    }
}
```

**Akzeptanzkriterien Phase 3:**
- [ ] Alle Stores werden bei `DB_VERSION = 2` angelegt
- [ ] `save_run` → `get_all_runs` gibt denselben Datensatz zurück (Roundtrip-Test im Browser)
- [ ] `set_setting("player_name", "Mordekai")` → `get_setting("player_name")` → `Some("Mordekai")`
- [ ] Migration von v1 → v2 löscht keine bestehenden Daten

---

## 6. Phase 4 – Services

### 6.1 `run_service.rs`

**Datei:** `src/services/run_service.rs`

Verantwortlich für Run-Lifecycle. Kein UI-State, keine IDB-Zugriffe direkt – nur Koordination.

```rust
pub enum RunState {
    Idle,
    Configured(RunConfig),
    Running { config: RunConfig, started_at: u64 },
    Paused  { config: RunConfig, started_at: u64 },
    Stopped { config: RunConfig, started_at: u64, stopped_at: u64 },
    Saved,
}

pub struct RunService {
    pub state: RwSignal<RunState>,
    pub stats: RwSignal<Stats>,
    pub offset: RwSignal<u64>,
}

impl RunService {
    pub fn configure(&self, config: RunConfig);
    pub fn start(&self, file_size: u64);     // setzt offset=file_size, stats=default, state=Running
    pub fn pause(&self);
    pub fn resume(&self);
    pub fn skip_to_now(&self, file_size: u64);  // BUG-002 Fix
    pub fn stop(&self);
    pub async fn save(&self, db: &Db, mu_map: &MuMap,
                      weapon: &Weapon, amp: Option<&Amplifier>,
                      armor: Option<&ArmorProfile>, fap: Option<&FapProfile>)
        -> Result<SavedRun, JsValue>;
    pub fn discard(&self);
}
```

**Kostenberechnung in `save()`:**

```
ammo_cost_ped     = stats.total_shots × pec_per_shot / 100
armor_decay_ped   = stats.total_damage_taken × armor.repair_pec_per_damage_point / 100  (wenn armor gesetzt)
fap_decay_ped     = stats.total_heal_self × fap.pec_per_heal_point / 100  (wenn fap gesetzt)
total_cost_ped    = ammo + armor + fap
profit_ped        = stats.total_loot_value_ped - total_cost_ped
return_pct_tt     = (stats.total_loot_value_ped / total_cost_ped) * 100
```

### 6.2 `analytics/stats.rs` erweitern

Ergänze `apply_event()` für alle neuen Event-Typen:

```rust
Event::SkillGain { skill, amount } => {
    *self.skill_gains.entry(skill.clone()).or_insert(0.0) += amount;
}
Event::AttributeGain { attribute, amount } => {
    *self.attribute_gains.entry(attribute.clone()).or_insert(0.0) += amount;
}
Event::HealSelf { amount } => {
    self.total_heal_self += amount;
}
Event::DamageTaken { amount } => {
    self.total_damage_taken += amount;
}
Event::GlobalKill(g) => {
    self.globals.push(g.clone());
}
```

Kill-Maturity-Erkennung in `record_kill()`:

```rust
pub fn record_kill(&mut self, kill_damage: f64, creature_config: Option<&CreatureConfig>) {
    self.kills += 1;
    if let Some(cfg) = creature_config {
        if let Some(mat) = cfg.match_maturity(kill_damage) {
            *self.kills_by_maturity.entry(mat.name.clone()).or_insert(0) += 1;
        }
    }
}
```

### 6.3 `drop_index.rs`

```rust
pub fn build_drop_index(runs: &[SavedRun]) -> DropIndex {
    // Für jede Kreatur: für jedes gelo otete Item:
    //   drop_rate = (Anzahl Kills mit diesem Item) / (Gesamtkills der Kreatur)
    //   avg_tt_per_run = Summe TT-Wert / Anzahl Runs
}
```

**Akzeptanzkriterien Phase 4:**
- [ ] `RunService::start()` setzt Offset auf Dateiende (kein erneutes Lesen alter Zeilen)
- [ ] `RunService::save()` berechnet alle Kostenpositionen korrekt
- [ ] `apply_event` aggregiert alle 10 Event-Typen (manuelle Verifikation mit chat_example.txt)

---

## 7. Phase 5 – Run-Panel (UI)

**Datei:** `src/components/run_panel.rs`
**Ersetzt:** `src/components/log_tailer.rs` (kann nach Migration gelöscht werden)

### 7.1 App-Shell mit Tab-Navigation

```
src/components/app_shell.rs:
  - Tab-Leiste: 🎯 Run | 🐾 Kreaturen | 🔫 Loadouts | 📊 History | 📈 Analyse | 🔨 Crafting | ⚙️ Einstellungen
  - Aktiver Tab als RwSignal<Tab>
  - Rendert je nach Tab die richtige Panel-Komponente
```

### 7.2 Run-Panel Lifecycle-UI

```
State IDLE (kein File-Handle):
  [Log-Datei wählen]

State IDLE (File-Handle vorhanden):
  [Run konfigurieren]

State CONFIGURED:
  Zeige RunConfig-Zusammenfassung
  [Los!]

State RUNNING:
  Live-Statistiken (s. 7.3)
  [Pause]

State PAUSED:
  Live-Statistiken (ausgegraut)
  [Weiter]  [Skip to now]  [Run beenden]

State STOPPED:
  Abschluss-Zusammenfassung
  [Speichern]  [Verwerfen]  [Neuer Run]

State SAVED:
  "Run gespeichert ✅"
  [Neuer Run]
```

### 7.3 Run-Konfiguration (Modal oder Inline-Form)

Felder:
- **Kreatur** – Autocomplete aus CreatureConfig-DB (Freitext wenn nicht in DB)
- **Loadout** – Dropdown aus Loadouts-DB (Pflichtfeld)
- **Session-Budget** – optionales Zahlenfeld in PED
- **Budget-Warnschwelle** – optionaler Slider 50–100%
- **Rüstung** – optionaler Dropdown aus ArmorProfile-DB
- **FAP** – optionaler Dropdown aus FapProfile-DB
- **Ziel-HP-Notiz** – optionales Freitext-Feld

### 7.4 Live-Statistiken

Tabelle mit folgenden Zeilen:

| Zeile | Berechnung |
|---|---|
| Kills gesamt | `stats.kills` |
| Kills je Maturity | `stats.kills_by_maturity` (aufklappbar, wenn CreatureConfig geladen) |
| Schüsse | `stats.total_shots` |
| Crits | `stats.player_crit_hits` |
| Crit-Rate | `crits / shots * 100` % |
| Evades (Spieler) | `stats.player_evades` |
| Misses (Spieler) | `stats.player_misses` |
| Gesamtschaden | `stats.total_damage` pts |
| Ø Schaden/Schuss | `total_damage / shots` |
| Ammo-Kosten | `shots × pec_per_shot / 100` PED |
| Loot TT | `stats.total_loot_value_ped` PED |
| Return % (TT) | `loot_tt / ammo_cost * 100` % |
| Gewinn/Verlust | `loot_tt - ammo_cost` PED |
| **Kosten/Kill** | `ammo_cost / kills` PED ← BUG-001 Fix |
| Schaden genommen | `stats.total_damage_taken` pts |
| Selbstheilung | `stats.total_heal_self` pts |
| Globals | `stats.globals.len()` |
| Skill-Gains | Aufklappbare Liste (skill → amount) |

### 7.5 Budget-Anzeige

Fortschrittsbalken:
- Grün: < warn_pct verbraucht
- Gelb: ≥ warn_pct
- Rot: ≥ 100%

```
Budget: 50 PED  [████████░░░░░░░░░░░░] 42% (21.00 PED verbraucht)
```

**Akzeptanzkriterien Phase 5:**
- [ ] Alle 6 Run-Zustände rendern korrekt
- [ ] "Skip to now" setzt Offset, ohne Stats zurückzusetzen
- [ ] Budget-Balken wechselt Farbe korrekt
- [ ] Kosten/Kill = ammo_cost / kills (nicht / loot_events)
- [ ] Beim Start werden keine alten Log-Zeilen gelesen

---

## 8. Phase 6 – Creatures-Panel (UI)

**Datei:** `src/components/creatures_panel.rs`

### 8.1 Kreatur-Liste

- Alle Kreaturen aus IDB laden
- Tabellarisch: Name | Maturities (komma-separiert) | Aktionen
- Sortiert alphabetisch

### 8.2 Kreatur anlegen / bearbeiten

Formular:
- **Kreatur-Name** (String, eindeutig)
- **Maturities** (dynamische Liste):
  - Maturity-Name (z.B. "Young")
  - HP Min (f64)
  - HP Max (f64)
  - [+] / [-] Buttons zum Hinzufügen/Entfernen

Validierung:
- Kreatur-Name darf nicht leer sein
- HP Max > HP Min
- Mindestens 1 Maturity

### 8.3 Kreatur löschen

Bestätigungsdialog: „Kreatur X löschen? Diese Aktion kann nicht rückgängig gemacht werden."

**Akzeptanzkriterien Phase 6:**
- [ ] Kreatur speichern → erscheint in Liste und im Run-Panel-Autocomplete
- [ ] Maturity-HP-Ranges werden korrekt persistiert
- [ ] Löschen entfernt Eintrag aus IDB

---

## 9. Phase 7 – Loadouts-Panel (UI)

**Datei:** `src/components/loadouts_panel.rs`

### 9.1 Sub-Tabs

```
[Waffen] [Amplifier] [Rüstungen] [FAPs] [Loadouts]
```

### 9.2 Waffen-Formular

- Name (String)
- Schaden Min (f64)
- Schaden Max (f64)
- PEC/Schuss (f64)

### 9.3 Amplifier-Formular

- Name (String)
- Flat-Schadensbonus (f64)
- Decay PEC/Schuss (f64)

### 9.4 Rüstungs-Formular

- Name (String)
- Reparatur PEC/Schadenspunkt (f64)

### 9.5 FAP-Formular

- Name (String)
- PEC/geheilten HP-Punkt (f64)

### 9.6 Loadout-Formular

- Name (String)
- Waffe (Dropdown aus Weapon-DB, Pflicht)
- Amplifier (Dropdown, optional)

Zeige Zusammenfassung:
```
Ø Schaden: 27.0 pts  |  Dmg-Range: 14–40  |  PEC/Shot: 8.50
```

**Akzeptanzkriterien Phase 7:**
- [ ] Loadout-Dropdown im Run-Panel zeigt alle gespeicherten Loadouts
- [ ] PEC/Shot-Berechnung ist korrekt (Waffe + Amp)

---

## 10. Phase 8 – History-Panel (UI)

**Datei:** `src/components/history_panel.rs`

### 10.1 Run-Liste

Lädt alle Runs aus IDB, sortiert nach Datum absteigend.

Filterleiste:
- Kreatur (Dropdown, Multi-Select)
- Zeitraum (Von-Datum, Bis-Datum)

Pro Run (komprimierte Zeile, aufklappbar):

```
2026-03-21 19:00  |  Atrox Young  |  42 Kills  |  Kosten: 21.50 PED  |  Loot: 20.30 PED  |  Return: 94.4%  |  G/V: -1.20 PED
```

Aufgeklappt zeigt:
- Maturity-Verteilung
- Schüsse, Crits, Evades, Misses
- Armor-Decay, FAP-Decay
- Loot-Tabelle (Items + MU%)
- Skill-Gains
- Globals/HOF

### 10.2 Aggregations-Zeile

Summen und Durchschnitte über alle gefilterten Runs:
- Kills gesamt, Ø Kills/Run
- Gesamtkosten, Gesamtloot
- Ø Return %
- Gesamtgewinn/-verlust

### 10.3 Kreatur-ROI-Ranking

Gruppiert nach Kreatur über alle Runs:

```
1. Atrox Young    12 Runs | Ø Return 94.2% | Ø Kills/h 48 | G/V gesamt: -2.40 PED
2. Drone Scout     5 Runs | Ø Return 91.5% | Ø Kills/h 65 | G/V gesamt: -3.10 PED
```

Kills/h = `kills / (duration_seconds / 3600)`

### 10.4 Tageszeit-Analyse

Return % gruppiert nach Stunde des Tages (0–23), Bar-Chart (ASCII oder Leptos-Chart).
Hinweis unter dem Chart: „Deine besten Runs waren XX:00–XX:00 Uhr".

### 10.5 FAP-Effizienz-Vergleich

Wenn mehrere FAP-Profile in Runs verwendet wurden:

| FAP | Ø PEC/HP | Ø Kosten/Run |
|---|---|---|
| FAP-50 | 2.40 | 0.84 PED |
| FAP-90 | 1.80 | 1.20 PED |

**Akzeptanzkriterien Phase 8:**
- [ ] Filter nach Kreatur und Datum funktioniert
- [ ] Aggregationszeile aktualisiert sich bei Filteränderung
- [ ] ROI-Ranking ist korrekt sortiert

---

## 11. Phase 9 – Analyse-Panel (UI)

**Datei:** `src/components/analyse_panel.rs`

### 11.1 Loadout-Optimizer

Eingabe:
- Kreatur (Dropdown)
- Maturity (Dropdown, abhängig von Kreatur)

Tabelle über alle Loadouts:

| Loadout | Ø Dmg | PEC/Shot | Schüsse/Kill | Ø Overkill | Kosten/Kill |
|---|---|---|---|---|---|
| LR-40 solo | 27.0 | 8.50 | 12 | 24 pts | 1.02 PED |
| LR-40 + AS-101 | 39.0 | 11.70 | 8 | 12 pts | **0.94 PED ✅** |

Berechnungen:
```
hp_midpoint    = (maturity.hp_min + maturity.hp_max) / 2
shots_per_kill = round(hp_midpoint / loadout.avg_damage())
overkill_pts   = (shots_per_kill × avg_damage) - hp_midpoint
kosten_per_kill = shots_per_kill × pec_per_shot / 100
```

Empfehlung: Loadout mit geringstem `kosten_per_kill` wird fettgedruckt mit ✅ hervorgehoben.

**Implementierung:** `src/analytics/optimizer.rs`

```rust
pub struct LoadoutScore {
    pub loadout_name: String,
    pub avg_damage: f64,
    pub pec_per_shot: f64,
    pub shots_per_kill: u64,
    pub avg_overkill_pts: f64,
    pub cost_per_kill_ped: f64,
}

pub fn score_loadouts(
    maturity: &Maturity,
    loadouts: &[(Loadout, Weapon, Option<Amplifier>)],
) -> Vec<LoadoutScore> { ... }
```

### 11.2 Loot-Histogramm

Aus `kill_loots` aller geladenen Runs (gefiltert nach Kreatur):

Buckets: `0.00–0.50` | `0.50–1.00` | `1.00–2.00` | `2.00–5.00` | `5.00+`

```
0.00–0.50 PED: ████████████████ 42 Kills
0.50–1.00 PED: ████████ 21 Kills
1.00–2.00 PED: ████ 11 Kills
5.00+ PED:     █ 2 Kills (Global!)
```

Einfache CSS-Bar-Chart-Darstellung (keine externe Chart-Lib nötig).

### 11.3 Loot-Streak / Trockenphase

Aus dem aktiven Run:
- Konfigurierbare Schwelle (Standard: Ø Loot/Kill des Runs)
- Laufende Summe PED-Ausgaben seit letztem Loot über Schwelle
- Längste bisherige Trockenphase (PED-Ausgaben)
- Farb-kodierte Kill-Sequenz: `🟢🔴🔴🟢🟢🔴` (grün = über Ø, rot = unter Ø)

### 11.4 Kills/Stunde & Kill-Zeit

Aus `kill_timestamps` des aktiven Runs:
- Kills/h = kills / (elapsed_seconds / 3600)
- Ø Kill-Zeit = `(last_kill_ts - first_kill_ts) / (kills - 1)`
- Min/Max Kill-Zeit

### 11.5 Survival-Rate

- Dodge-Rate = `player_evades / (player_hits + player_evades)` %
- Ø Schaden genommen/Kill
- Ø Heilung/Kill

**Akzeptanzkriterien Phase 9:**
- [ ] Optimizer zeigt korrekte Berechnungen (manuell verifizieren)
- [ ] Histogramm-Buckets summieren korrekt auf Gesamtzahl Kills

---

## 12. Phase 10 – Crafting-Panel (UI)

**Datei:** `src/components/crafting_panel.rs`

### 12.1 Blueprint-Liste

Alle Blueprints aus IDB, tabellarisch mit Name und Output.

### 12.2 Blueprint-Formular

- Name (String)
- Output-Item, Output-Menge, Output-TT-Wert, Output-MU%
- Zutaten (dynamische Liste):
  - Item-Name, Menge, TT/Einheit, MU%
  - MU% wird automatisch aus globaler MU-Map vorausgefüllt (wenn Item-Name bekannt)
  - Änderungen an MU% hier aktualisieren die globale MU-Map

### 12.3 Wirtschaftlichkeitsrechnung

Live-Berechnung beim Blueprint-Bearbeiten:

```
Zutat          | Menge | TT/Unit | MU%  | Kosten
───────────────────────────────────────────────
Shrapnel       |   500 |  0.0001 | 101% |  0.051 PED
Robot Residue  |    20 |  0.0100 | 115% |  0.230 PED
───────────────────────────────────────────────
Input gesamt                              0.281 PED
Output (TT)                               0.510 PED
Output (+MU%)                             0.620 PED
Gewinn/Verlust                           +0.339 PED ✅
```

`kosten_item = qty × tt_per_unit × (mu_pct / 100)`
`output_with_mu = output_tt × (output_mu_pct / 100)`

### 12.4 Kreatur-Empfehlung

Für den ausgewählten Blueprint aus dem Drop-Index (`build_drop_index(runs)`):

```
✅ Drone Scout  3/3 Zutaten ⭐⭐⭐  Effizienz: 0.82  (12 Runs)
   Shrapnel ✓ (95% Drop-Rate)  Robot Residue ✓ (40%)  Tier 2 Comp ✓ (15%)

⚡ Atrox Young  2/3 Zutaten ⭐⭐  (8 Runs)
   Shrapnel ✓ (88%)  Robot Residue ✗  Tier 2 Comp ✓ (22%)
```

Scoring:
1. Item-Coverage: `gefundene_zutaten / benötigte_zutaten` → Primärkriterium
2. Drop-Effizienz: `Σ(drop_rate × avg_tt_per_run) / total_hunting_cost` → Sekundärkriterium
3. Vollständigkeits-Bonus: Coverage = 100% bevorzugt

**Akzeptanzkriterien Phase 10:**
- [ ] MU-Änderung im Blueprint-Formular spiegelt sich im Run-Panel wider (gleiche MU-Map)
- [ ] Kreatur-Empfehlung erscheint nur wenn Drop-Index Daten aus Runs vorhanden sind

---

## 13. Phase 11 – Einstellungen-Panel (UI)

**Datei:** `src/components/settings_panel.rs`

### 13.1 Charakter-Name

- Textfeld mit Speichern-Button
- Beim ersten App-Start (player_name nicht gesetzt): Modal/Prompt vor dem Rest der App
- Wird für Global-Filterung genutzt (nur Globals des eigenen Charakters zählen)

### 13.2 Export

Button „Backup exportieren" → triggert Browser-Download:

Dateiname: `logtailer_backup_YYYY-MM-DD.json`

```json
{
  "version": 1,
  "exported_at": "2026-03-21T19:00:00Z",
  "player_name": "Mordekai Azrael",
  "runs": [...],
  "creatures": [...],
  "weapons": [...],
  "amps": [...],
  "armor_profiles": [...],
  "fap_profiles": [...],
  "loadouts": [...],
  "blueprints": [...],
  "mu_map": { "Shrapnel": 101.0, ... }
}
```

**Implementierung:** `src/persistence/export.rs`

```rust
pub async fn export_all(db: &Db) -> Result<String, JsValue>;  // → JSON String
pub fn trigger_download(filename: &str, json: &str);           // Blob + <a download>
```

### 13.3 Import

1. `<input type="file" accept=".json">` → liest JSON
2. Vorschau: „X Runs, Y Kreaturen, Z Blueprints werden importiert"
3. Bei ID-Konflikt: Auswahl „Überschreiben" / „Überspringen"
4. Bestätigungsbutton → schreibt alle Objekte in IDB

```rust
pub async fn import_all(db: &Db, json: &str, on_conflict: ConflictStrategy) -> Result<ImportSummary, JsValue>;

pub enum ConflictStrategy { Overwrite, Skip }

pub struct ImportSummary {
    pub runs_imported: u64,
    pub creatures_imported: u64,
    pub blueprints_imported: u64,
    // ...
}
```

### 13.4 Daten zurücksetzen

Button „Alle Daten löschen" → Bestätigungsdialog → `indexedDB.deleteDatabase("log_tailer")` + Seite neu laden.

**Akzeptanzkriterien Phase 11:**
- [ ] Export-JSON ist valides JSON und enthält alle Stores
- [ ] Import-Roundtrip: Export → Import → gleiche Daten in IDB
- [ ] Charakter-Name wird beim Start geprüft; Modal erscheint wenn nicht gesetzt

---

## 14. Bug-Fixes

### BUG-001: Kosten pro Kill falsch

**Datei:** `src/components/run_panel.rs` (live stats) + `src/services/run_service.rs` (bei save)

**Problem:** Aktuell wird durch die Anzahl der Loot-Events dividiert.

**Fix:**
```rust
// Falsch:
let cost_per_kill = ammo_cost_ped / stats.total_loot_value_ped;

// Richtig:
let cost_per_kill = if stats.kills > 0 {
    ammo_cost_ped / stats.kills as f64
} else {
    0.0
};
```

**Wo:** In der Live-Statistiken-Berechnung im Run-Panel und in `RunService::save()`.

---

### BUG-002: Pause-Problem (Offset nicht aktualisiert)

**Datei:** `src/services/tailer.rs` + `src/services/run_service.rs`

**Problem:** Beim Pausieren bleibt der Offset stehen. Beim Resume werden alle Log-Zeilen die während der Pause entstanden (Crafting, Chat, etc.) dem laufenden Run zugerechnet.

**Fix – „Skip to now"-Button:**

```rust
// In RunService:
pub async fn skip_to_now(&self, file_handle: &JsValue) {
    // 1. Aktuelle Dateigröße ermitteln (via fs_access::get_file_size)
    // 2. offset.set(file_size) → künftige Polling-Iteration startet ab hier
    // 3. Stats werden NICHT zurückgesetzt
}
```

Der Button erscheint nur im Zustand `PAUSED`:
```
[Weiter]  [Skip to now]  [Run beenden]
```

`tailer.rs`: Der Polling-Loop liest `offset.get()` am Anfang jeder Iteration → der neue Offset greift automatisch beim nächsten Tick.

---

## 15. Nicht-funktionale Anforderungen

### PWA / Offline

- `public/service-worker.js` muss alle App-Assets (WASM, JS, CSS, Icons) cachen
- Precache-Strategie: Cache-first für App-Shell, Network-first für nichts (kein Backend)
- Service-Worker-Version hochsetzen bei jedem Release (damit Updates greifen)

### Performance

- Polling-Loop läuft in `spawn_local` (nicht blockierend)
- IDB-Schreibvorgänge werden gebündelt (pro Polling-Tick 1 Transaktion, nicht eine pro Event)
- Große Log-Chunks (> 10.000 Zeilen) müssen ohne UI-Freeze verarbeitet werden
  - Lösung: `gloo_timers::future::TimeoutFuture::new(0).await` nach je 1.000 Zeilen als Yield-Punkt

### Datei-Zugriff

- Persistent Handle via File System Access API: Handle wird in `sessionStorage` referenziert (nicht in IDB, da nicht serialisierbar)
- Fallback `<input type="file">`: Kein persistenter Handle möglich → Handle geht bei Page-Refresh verloren

### Browser-Kompatibilität

- Ziel: Chrome/Edge (Chromium) – Full File System Access API Support
- Firefox-Fallback: `<input>` Picker (kein persistenter Handle)

---

## 16. Akzeptanzkriterien (Gesamt)

Ein vollständiger Integrations-Test mit der `chat_example.txt`:

1. App lädt ohne Fehler in der Browser-Konsole
2. Log-Datei wählen → Handle wird gespeichert
3. Run konfigurieren: Kreatur = „Atrox Young" (aus CreatureConfig mit Young: 100–200 HP)
4. Loadout wählen: Waffe mit 20 PEC/Shot
5. Run starten → Offset springt ans Dateiende
6. Manuell Testzeilen an chat_example.txt anhängen (PlayerHit, Loot, SkillGain, ...)
7. Nach 1–2 Sekunden: Live-Statistiken aktualisieren sich korrekt
8. Pause → Testzeilen anhängen → "Skip to now" → Resume → neue Zeilen erscheinen nicht in Stats
9. Run beenden → Speichern → erscheint in History
10. History: Kreatur-Filter auf „Atrox Young" zeigt nur diesen Run
11. Analyse: Loadout-Optimizer zeigt korrekten Kosten/Kill für das gewählte Loadout
12. Crafting: Blueprint anlegen, Kreatur-Empfehlung erscheint basierend auf gespeichertem Run
13. Einstellungen: Export → JSON Download → Import → gleiche Daten in IDB
14. Offline: App-Tab neu laden ohne Netzwerkverbindung → App lädt vollständig aus Service-Worker-Cache

---

*Ende der Spec – Phase 1 beginnt mit `src/domain/types.rs`.*
