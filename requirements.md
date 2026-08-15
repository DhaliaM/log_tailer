# log_tailer – Requirements

**Stand:** 2026-03-21
**Spiel:** Entropia Universe
**Plattform:** Browser-PWA (Rust/WASM/Leptos, IndexedDB, kein Backend)

---

## Überblick

log_tailer liest die Entropia Universe `chat.log` inkrementell im Browser aus und wertet
Kampf-Events in Echtzeit aus. Ziel ist eine vollständige Hunting-Analyse-Suite mit
Run-Tracking, Loadout-Optimierung, Crafting-Planung und Datenpersistenz.

---

## Log-Format (Entropia Universe `chat.log`)

Alle relevanten Zeilen tragen das Präfix `[System] []`:

```
2026-03-21 19:10:17 [System] [] You inflicted 110.5 points of damage
2026-03-21 19:10:41 [System] [] Critical hit - Additional damage! You inflicted 216.2 points of damage
2026-03-21 19:10:23 [System] [] The target Dodged your attack
2026-03-21 19:10:37 [System] [] You Dodged the attack
2026-03-21 19:10:41 [System] [] The attack missed you
2026-03-21 19:10:35 [System] [] You took 21.5 points of damage
2026-03-21 19:10:17 [System] [] You healed yourself 2.2 points
2026-03-21 19:10:19 [System] [] You have gained 0.0150 experience in your Gauss Weaponry Technology skill
2026-03-21 19:10:54 [System] [] You have gained 0.0334 Dexterity
2026-03-21 19:10:33 [System] [] You received Shrapnel x (2936) Value: 0.2936 PED
2026-03-21 19:11:08 [Globals] [] Team "(Shared Loot)" killed a creature (KING KONG) with a value of 582 PED!
```

Ignoriert werden: `[Rookie]`, `[Globals]` (außer eigene), `[#kanal]`, Login-Meldungen.

---

## Bekannte Bugs (zu beheben)

### BUG-001: Kosten pro Kill falsch
- **Problem:** Division durch Loot-Events statt durch Kill-Anzahl → Wert zu niedrig
- **Fix:** `kosten_ped / kills` (mit Guard für `kills == 0`)

### BUG-002: Pause-Problem
- **Problem:** Beim Stopp/Pause wird der Datei-Offset nicht aktualisiert. Alle
  Log-Zeilen die während der Pause entstehen (andere Aktivitäten, Crafting, etc.)
  werden beim nächsten Resume fälschlicherweise dem laufenden Run zugerechnet.
- **Fix:** Neuer „Skip to now"-Button setzt den Offset auf das aktuelle Dateiende,
  ohne die Run-Statistiken zu verändern.

---

## Zu parsende Log-Events

| Event | Log-Zeile | Verwendung |
|---|---|---|
| `PlayerHit` | `You inflicted X points of damage` | Schaden, Schuss-Zählung |
| `PlayerHitCrit` | `Critical hit - Additional damage! You inflicted X` | Crit-Zählung |
| `EnemyEvaded` | `The target Dodged/Evaded your attack` | Trefferquote |
| `EnemyMiss` | `The attack missed you` | Defensive Stats |
| `PlayerEvaded` | `You Dodged/Evaded the attack` | Defensive Stats |
| `PlayerMiss` | `You missed` | Trefferquote |
| `Loot` | `You received ITEM x (QTY) Value: X PED` | Loot-Tracking, Kill-Erkennung |
| `SkillGain` | `You have gained X experience in your SKILL skill` | Skill-Tracking (informativ) |
| `AttributeGain` | `You have gained X Dexterity` | Attribut-Tracking (informativ) |
| `HealSelf` | `You healed yourself X points` | FAP-Nutzung |
| `DamageTaken` | `You took X points of damage` | Armor-Beanspruchung |
| `GlobalKill` | `[Globals] ... killed a creature ... value of X PED` | Eigene Globals/HOF |

**Kill-Erkennung:** Gruppe von Loot-Zeilen mit gleichem Sekunden-Timestamp = 1 Kill.
**Skill-Gains:** Nur informativ getrackt, kein PED-Wert, fließen **nicht** in Return/ROI ein.
**Globals:** Nur eigene Globals werden gezählt (Charakter-Name konfigurierbar).

---

## App-Struktur: 7 Panels

| Panel | Beschreibung |
|---|---|
| 🎯 **Run** | Aktiver Hunt: Konfiguration, Start/Pause/Stop, Live-Statistiken |
| 🐾 **Kreaturen** | Datenbank aller Kreaturen mit Maturities und HP-Ranges |
| 🔫 **Loadouts** | Waffen, Amplifier, Rüstungen, FAPs und Loadout-Kombinationen |
| 📊 **History** | Archiv aller gespeicherten Runs mit Filter und Auswertung |
| 📈 **Analyse** | Loot-Histogramm, Streak-Visualisierung, Loadout-Optimizer |
| 🔨 **Crafting** | Blueprint-Datenbank mit Wirtschaftlichkeit und Kreatur-Empfehlung |
| ⚙️ **Einstellungen** | Charakter-Name, Backup Export/Import, Daten zurücksetzen |

---

## Panel: 🎯 Run

### Run-Konfiguration (vor dem Start)
- Kreatur auswählen (aus Kreatur-Datenbank, Autocomplete)
- Loadout auswählen (aus Loadout-Datenbank) → PEC/Shot wird automatisch berechnet
- Session-Budget optional: Betrag in PED + Warnschwelle in %
- Optionale Ziel-HP-Notiz (freies Textfeld)

### Run-Lifecycle

```
IDLE → CONFIGURED → RUNNING ⟷ PAUSED → STOPPED → SAVED → IDLE
```

| Zustand | Sichtbare Aktionen |
|---|---|
| Kein Handle | [Log-Datei wählen] |
| Handle, IDLE | [Run konfigurieren] |
| CONFIGURED | [Los!] |
| RUNNING | [Pause] |
| PAUSED | [Weiter] [Skip to now] [Run beenden] |
| STOPPED | [Speichern] [Verwerfen] [Neuer Run] |

**„Skip to now":** Setzt Datei-Offset auf aktuelles Dateiende. Run-Stats bleiben erhalten.
Damit werden Log-Zeilen die während der Pause entstanden sind ignoriert.

### Live-Statistiken (während Run)

- Kills gesamt + Aufschlüsselung nach Maturity (wenn CreatureConfig geladen)
- Schüsse, Crits, Evades, Misses
- Gesamtschaden, Ø Schaden/Schuss
- Ammo-Kosten PED (aus Loadout: Schüsse × PEC/Shot)
- Loot TT-Wert PED + Loot mit MU%
- Return % (TT) + Return % (mit MU)
- Gewinn / Verlust PED
- **Kosten pro Kill** (korrigierter Bug-001: `kosten / kills`)
- Ø Overkill/Kill (Pts + PED-Kosten) wenn Maturity bekannt
- Skill-Gains pro Skill (aufklappbar, informativ)
- FAP geheilt (pts) + Schaden erhalten (pts)
- Budget-Anzeige mit Fortschrittsbalken (grün/gelb/rot)

### Run-Abschluss

Beim Speichern wird berechnet:
- `ammo_cost_ped = total_shots × pec_per_shot / 100`
- `armor_decay_ped = total_damage_taken × armor.repair_cost_per_damage_point / 100` (optional)
- `fap_decay_ped = total_heal_self × fap.pec_per_heal_point / 100` (optional)
- `total_cost_ped = ammo + armor + fap`
- `profit_ped = total_loot_ped - total_cost_ped`

---

## Panel: 🐾 Kreaturen

Zentrale Datenbank aller Kreaturen. Wird in IndexedDB gespeichert und
bei Run-Start sowie im Crafting-Panel genutzt.

### Funktionen
- Kreatur anlegen / bearbeiten / löschen
- Pro Kreatur: beliebig viele Maturities mit HP-Min und HP-Max
- Beispiel: Atrox → Young (100–200 HP), Mature (200–350 HP), Old (350–500 HP)

### Wozu HP-Ranges?
- **Maturity-Erkennung:** Damage-pro-Kill wird gegen HP-Ranges gematcht
  → System erkennt automatisch welche Maturity getötet wurde
- **Overkill-Berechnung:** Overkill = Kill-Damage − HP-Midpoint
- **Loadout-Optimizer:** Berechnet optimalen Schaden/Schuss für jede Maturity

---

## Panel: 🔫 Loadouts

Verwaltung aller Ausrüstungs-Profile. Einmalig konfigurieren, dann per Dropdown
bei jedem Run auswählen.

### Waffen-Profil
- Name (z.B. „ArMatrix LR-40 (L)")
- Schaden Min/Max
- PEC pro Schuss (Ammo-Kosten)

### Amplifier-Profil
- Name (z.B. „AS-101 (L)")
- Flat-Schadensbonus pro Schuss
- Decay PEC pro Schuss

### Rüstungs-Profil
- Name (z.B. „Gremlin Set")
- Reparaturkosten in PEC pro erhaltenem Schadenspunkt

### FAP-Profil
- Name (z.B. „FAP-50")
- Kosten in PEC pro geheiltem HP-Punkt

### Loadout
- Name
- Waffe (Pflicht) + Amplifier (optional)
- Angezeigte Zusammenfassung: Ø Schaden, Dmg-Range, PEC/Shot gesamt

Ein Run referenziert ein Loadout → PEC/Shot wird automatisch berechnet,
kein manuelles Eintippen mehr nötig.

---

## Panel: 📊 History

Archiv aller gespeicherten Runs.

### Run-Liste
Sortiert nach Datum, filterbar nach Kreatur und Zeitraum.

Pro Run:
- Datum, Kreatur, Maturity-Verteilung, Laufzeit
- Kills gesamt + pro Maturity
- Schüsse, Crits, Evades
- Ammo-Kosten, Armor-Decay, FAP-Decay, Gesamtkosten (PED)
- Loot TT + mit MU%, Return %, Gewinn/Verlust
- Loot-Tabelle (Items + MU%)
- Skill-Gains pro Skill
- Globals/HOF des Runs

### Aggregations-Zeile
Summen/Durchschnitte über alle gefilterten Runs.

### Kreatur-ROI-Ranking
Über alle Runs gruppiert nach Kreatur:
```
1. Atrox Young    12 Runs | Ø Return 94.2% | Ø Kills/h 48 | G/V gesamt: -2.40 PED
2. Drone Scout     5 Runs | Ø Return 91.5% | Ø Kills/h 65 | G/V gesamt: -3.10 PED
```

### Tageszeit-Analyse
Return % gruppiert nach Stunde des Tages (aus Run-Timestamps).
Hinweis: „Deine besten Runs waren morgens (06–09 Uhr)".

### FAP-Effizienz-Vergleich
Wenn mehrere FAP-Profile über verschiedene Runs verwendet wurden:
PEC/HP-geheilt + Ø-Kosten/Run pro FAP-Profil.

---

## Panel: 📈 Analyse

### Loadout-Optimizer
Für eine gewählte Kreatur + Maturity werden alle konfigurierten Loadouts verglichen:

| Loadout | Ø Dmg | PEC/Shot | Schüsse/Kill | Ø Overkill | Kosten/Kill |
|---|---|---|---|---|---|
| LR-40 solo | 27.0 | 8.50 | 12 | 24 pts | 1.02 PED |
| LR-40 + AS-101 | 39.0 | 11.70 | 8 | 12 pts | **0.94 PED ✅** |

Empfehlung: Loadout mit geringstem Kosten/Kill wird hervorgehoben.
Optimaler Schaden/Schuss: `hp / round(hp / avg_dmg)` → minimiert Overkill.

### Loot-Histogramm
Verteilung der Loot-Werte pro Kill (aus gespeicherten Runs):
```
0.00–0.50 PED: ████████████████ 42 Kills
0.50–1.00 PED: ████████ 21 Kills
1.00–2.00 PED: ████ 11 Kills
5.00+ PED:     █ 2 Kills (Global!)
```

### Loot-Streak / Trockenphase
- Laufende Summe PED-Ausgaben seit letztem „gutem" Loot (konfigurierbarer Schwellwert)
- Längste bisherige Trockenphase
- Farb-kodierte Kill-Sequenz (grün = über Ø, rot = unter Ø)

### Kills/Stunde & Kill-Zeit
- Kills pro Stunde (aus aktiver Run-Laufzeit + Kill-Timestamps)
- Ø Kill-Zeit in Sekunden (Spawn-Effizienz)
- Kürzeste/längste Kill-Zeit

### Survival-Rate
- Dodge-Rate: `player_evades / player_attacks`
- Ø Schaden genommen pro Kill
- Ø Heilung pro Kill

---

## Panel: 🔨 Crafting

### Blueprint-Datenbank
Pro Blueprint:
- Name
- Zutaten: Item-Name + Menge + TT-Wert/Einheit + MU%
- Output: Item-Name + Stückzahl + TT-Wert + MU%

MU%-Werte synchronisieren sich automatisch mit der globalen MU-Map
(gleicher Item-Name = gleicher Wert in beiden Stellen).

### Wirtschaftlichkeitsrechnung
```
Zutat          | Menge | TT/Unit | MU%  | Kosten
───────────────────────────────────────────────
Shrapnel       |   500 |  0.0001 | 101% |  0.051 PED
Robot Residue  |    20 |  0.0100 | 115% |  0.230 PED
Tier 2 Comp    |     5 |  0.1400 | 108% |  0.756 PED
───────────────────────────────────────────────
Input gesamt                              1.037 PED
Output (TT)                               0.510 PED
Output (+MU%)                             0.620 PED
Gewinn/Verlust                           -0.417 PED ❌
```

### Kreatur-Empfehlung (aus Run-History)
Das System baut automatisch aus allen gespeicherten Runs einen Drop-Index auf
(`Kreatur → { Item → Drop-Rate, Ø TT/Run }`). Kein manuelles Pflegen nötig.

Scoring pro Kreatur für ein Blueprint:
1. Item-Coverage: Wie viele der benötigten Zutaten werden gedroppt? (0–100%)
2. Drop-Effizienz: Ø TT-Wert der relevanten Items pro PED Hunting-Kosten
3. Vollständigkeits-Bonus: Coverage = 100% → bevorzugt

```
✅ Drone Scout  3/3 Zutaten ⭐⭐⭐  Effizienz: 0.82  (12 Runs)
   Shrapnel ✓ (95% Drop-Rate)  Robot Residue ✓ (40%)  Tier 2 Comp ✓ (15%)

⚡ Atrox Young  2/3 Zutaten ⭐⭐  (8 Runs)
   Shrapnel ✓ (88%)  Robot Residue ✗  Tier 2 Comp ✓ (22%)
```

---

## Panel: ⚙️ Einstellungen

### Charakter-Name
- Einmalig konfigurierbar, in IDB gespeichert
- Wird für Global/HOF-Filterung verwendet (nur eigene Globals zählen)
- Beim ersten App-Start: Eingabe-Prompt wenn nicht gesetzt

### Export (Full Backup)
- Button „Backup exportieren" → Download `logtailer_backup_YYYY-MM-DD.json`
- Inhalt: Runs, Kreaturen, Waffen, Amps, Rüstungen, FAPs, Loadouts, Blueprints,
  MU-Map, Charakter-Name, Schema-Version

```json
{
  "version": 1,
  "exported_at": "2026-03-21T19:00:00Z",
  "player_name": "Mordekai Azrael",
  "runs": [...],
  "creatures": [...],
  "loadouts": [...],
  "blueprints": [...]
}
```

### Import (Restore)
- JSON-Datei hochladen → Vorschau: X Runs, Y Kreaturen, Z Blueprints ...
- Bei ID-Konflikt (Eintrag existiert bereits): Überschreiben oder Überspringen
- Bestätigung vor dem Einspielen

### Daten zurücksetzen
- Mit Bestätigungsdialog
- Löscht IndexedDB + Service-Worker-Cache (bestehende PWA-Logik)

---

## Datenpersistenz (IndexedDB)

| Store | Key | Inhalt |
|---|---|---|
| `runs` | `id` (Timestamp) | Gespeicherte Runs mit vollständigen Stats |
| `creature_configs` | `creature` (Name) | Maturity-HP-Tabellen pro Kreatur |
| `weapons` | `id` | Waffen-Profile |
| `amps` | `id` | Amplifier-Profile |
| `armor_profiles` | `id` | Rüstungs-Profile |
| `fap_profiles` | `id` | FAP-Profile |
| `loadouts` | `id` | Loadout-Kombinationen |
| `blueprints` | `id` | Crafting-Blueprints |
| `settings` | `key` | `player_name`, `mu_map`, `last_creature`, `pec_per_shot` |

Schema-Versioning mit Migration via `on_upgrade_needed`.

---

## Nicht-funktionale Anforderungen

- **Offline-fähig:** PWA mit Service Worker, alle Daten lokal in IndexedDB
- **Kein Backend:** Vollständig client-seitig (WASM im Browser)
- **Datei-Zugriff:** File System Access API (persistent Handle) mit `<input>`-Fallback
- **Polling-Intervall:** 1 Sekunde
- **Performance:** Kein merkliches Ruckeln auch bei großen Log-Chunks
- **Portabilität:** Daten via JSON-Export auf anderen Browser/Gerät übertragbar
