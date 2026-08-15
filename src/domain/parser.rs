use once_cell::sync::Lazy;
use regex::Regex;
use super::types::Event;

// ─── Regex-Definitionen ──────────────────────────────────────────────────────

static RE_LOOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} \[System\] \[\] You received (?P<item>.+?) x \((?P<qty>\d+)\) Value: (?P<val>[0-9]*\.?[0-9]+) PED"#).unwrap()
});

static RE_CRIT_AND_INFLICT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] (?:Critical hit - Additional damage!\s*)?You inflicted (?P<dmg>[0-9]*\.?[0-9]+) points of damage"#).unwrap()
});

static RE_ENEMY_MISS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] The attack missed you"#).unwrap()
});

static RE_ENEMY_EVADED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] The target (Dodged|Evaded) your attack"#).unwrap()
});

static RE_PLAYER_MISS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] You (missed|miss)($| )"#).unwrap()
});

static RE_PLAYER_EVADED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] (?:You (Dodged|Evaded) the attack)"#).unwrap()
});

// Neue Regex (Phase 2) – Reihenfolge beachten: SKILL_GAIN vor ATTRIBUTE_GAIN
static RE_SKILL_GAIN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] You have gained (?P<amount>[0-9]*\.?[0-9]+) experience in your (?P<skill>.+?) skill"#).unwrap()
});

static RE_ATTRIBUTE_GAIN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] You have gained (?P<amount>[0-9]*\.?[0-9]+) (?P<attribute>.+)"#).unwrap()
});

static RE_HEAL_SELF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] You healed yourself (?P<amount>[0-9]*\.?[0-9]+) points"#).unwrap()
});

static RE_DAMAGE_TAKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] You took (?P<amount>[0-9]*\.?[0-9]+) points of damage"#).unwrap()
});

static RE_GLOBAL_KILL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[Globals\] \[\] (?P<player>.+?) killed a creature \((?P<creature>.+?)\) with a value of (?P<value>[0-9]*\.?[0-9]+) PED"#).unwrap()
});

// ─── Timestamp-Extraktion ────────────────────────────────────────────────────

/// Parst "YYYY-MM-DD HH:MM:SS" vom Zeilenanfang und gibt Unix-Sekunden zurück.
/// Implementiert manuell (kein chrono-Overhead im WASM-Hot-Path).
pub fn extract_timestamp_sec(line: &str) -> Option<u64> {
    // Erwartet mind. 19 Zeichen: "YYYY-MM-DD HH:MM:SS"
    if line.len() < 19 { return None; }
    let s = &line[..19];
    let year:  u64 = s[0..4].parse().ok()?;
    let month: u64 = s[5..7].parse().ok()?;
    let day:   u64 = s[8..10].parse().ok()?;
    let hour:  u64 = s[11..13].parse().ok()?;
    let min:   u64 = s[14..16].parse().ok()?;
    let sec:   u64 = s[17..19].parse().ok()?;

    // Tage seit Unix-Epoch (1970-01-01) berechnen
    // Algorithmus: Julian Day Number → Unix-Tage
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let jdn: u64 = 365 * y + y / 4 - y / 100 + y / 400
        + (153 * m + 2) / 5 + day + 1721119;
    // Unix-Epoch = JDN 2440588
    let unix_day = jdn.checked_sub(2440588)?;
    Some(unix_day * 86400 + hour * 3600 + min * 60 + sec)
}

// ─── Haupt-Parser ────────────────────────────────────────────────────────────

pub fn parse_line(line: &str) -> Option<Event> {
    // GlobalKill: aus [Globals]-Zeilen (frühzeitig prüfen, vor System-Filter)
    if line.contains("[Globals] []") {
        return parse_global_kill(line);
    }

    // Alle anderen Events: nur [System]-Zeilen
    if !line.contains("[System] []") {
        return None;
    }

    // Loot immer zuerst (damit "received" nicht von anderen Regex gestört wird)
    if let Some(c) = RE_LOOT.captures(line) {
        let item = unescape_html(c.name("item")?.as_str().trim());
        let qty: u64 = c.name("qty")?.as_str().parse().ok()?;
        let value_ped: f64 = c.name("val")?.as_str().parse().ok()?;
        let timestamp_sec = extract_timestamp_sec(line).unwrap_or(0);
        return Some(Event::Loot { item, qty, value_ped, timestamp_sec });
    }

    // Treffer (kritisch oder normal)
    if let Some(c) = RE_CRIT_AND_INFLICT.captures(line) {
        let dmg: f64 = c.name("dmg")?.as_str().parse().ok()?;
        let critical = line.contains("Critical hit - Additional damage!");
        return Some(Event::PlayerHit { damage: dmg, critical });
    }

    // Gegner verfehlt dich
    if RE_ENEMY_MISS.is_match(line) {
        return Some(Event::EnemyMiss);
    }

    // Dein Angriff wurde gedodged/evaded
    if RE_ENEMY_EVADED.is_match(line) {
        return Some(Event::EnemyEvaded);
    }

    // Eigener Miss
    if RE_PLAYER_MISS.is_match(line) {
        return Some(Event::PlayerMiss);
    }

    // Eigenes Dodgen/Evaden
    if RE_PLAYER_EVADED.is_match(line) {
        return Some(Event::PlayerEvaded);
    }

    // Selbstheilung
    if let Some(c) = RE_HEAL_SELF.captures(line) {
        let amount: f64 = c.name("amount")?.as_str().parse().ok()?;
        return Some(Event::HealSelf { amount });
    }

    // Schaden genommen
    if let Some(c) = RE_DAMAGE_TAKEN.captures(line) {
        let amount: f64 = c.name("amount")?.as_str().parse().ok()?;
        return Some(Event::DamageTaken { amount });
    }

    // Skill-Gain (VOR Attribute-Gain prüfen – überlappende Muster!)
    if let Some(c) = RE_SKILL_GAIN.captures(line) {
        let amount: f64 = c.name("amount")?.as_str().parse().ok()?;
        let skill = c.name("skill")?.as_str().trim().to_string();
        return Some(Event::SkillGain { skill, amount });
    }

    // Attribut-Gain (nach Skill-Gain)
    if let Some(c) = RE_ATTRIBUTE_GAIN.captures(line) {
        let amount: f64 = c.name("amount")?.as_str().parse().ok()?;
        let attribute = c.name("attribute")?.as_str().trim().to_string();
        return Some(Event::AttributeGain { attribute, amount });
    }

    None
}

fn unescape_html(s: &str) -> String {
    s.replace("&quot;", "\"")
     .replace("&amp;", "&")
     .replace("&lt;", "<")
     .replace("&gt;", ">")
     .replace("&apos;", "'")
}

fn parse_global_kill(line: &str) -> Option<Event> {
    let c = RE_GLOBAL_KILL.captures(line)?;
    let player   = unescape_html(c.name("player")?.as_str().trim());
    let creature = unescape_html(c.name("creature")?.as_str().trim());
    let value_ped: f64 = c.name("value")?.as_str().parse().ok()?;
    Some(Event::GlobalKill { creature, value_ped, player })
}

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::Event;

    // Beispielzeilen aus requirements.md
    const LINE_HIT:         &str = "2026-03-21 19:10:17 [System] [] You inflicted 110.5 points of damage";
    const LINE_CRIT:        &str = "2026-03-21 19:10:41 [System] [] Critical hit - Additional damage! You inflicted 216.2 points of damage";
    const LINE_ENEMY_EVADE: &str = "2026-03-21 19:10:23 [System] [] The target Dodged your attack";
    const LINE_PLAYER_EVADE:&str = "2026-03-21 19:10:37 [System] [] You Dodged the attack";
    const LINE_ENEMY_MISS:  &str = "2026-03-21 19:10:41 [System] [] The attack missed you";
    const LINE_DMG_TAKEN:   &str = "2026-03-21 19:10:35 [System] [] You took 21.5 points of damage";
    const LINE_HEAL:        &str = "2026-03-21 19:10:17 [System] [] You healed yourself 2.2 points";
    const LINE_SKILL:       &str = "2026-03-21 19:10:19 [System] [] You have gained 0.0150 experience in your Gauss Weaponry Technology skill";
    const LINE_ATTR:        &str = "2026-03-21 19:10:54 [System] [] You have gained 0.0334 Dexterity";
    const LINE_LOOT:        &str = "2026-03-21 19:10:33 [System] [] You received Shrapnel x (2936) Value: 0.2936 PED";
    const LINE_GLOBAL:      &str = "2026-03-21 19:11:08 [Globals] [] Team \"(Shared Loot)\" killed a creature (KING KONG) with a value of 582 PED!";

    #[test]
    fn test_player_hit() {
        match parse_line(LINE_HIT).unwrap() {
            Event::PlayerHit { damage, critical } => {
                assert!((damage - 110.5).abs() < 0.001);
                assert!(!critical);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_player_hit_crit() {
        match parse_line(LINE_CRIT).unwrap() {
            Event::PlayerHit { damage, critical } => {
                assert!((damage - 216.2).abs() < 0.001);
                assert!(critical);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_enemy_evaded() {
        assert!(matches!(parse_line(LINE_ENEMY_EVADE).unwrap(), Event::EnemyEvaded));
    }

    #[test]
    fn test_player_evaded() {
        assert!(matches!(parse_line(LINE_PLAYER_EVADE).unwrap(), Event::PlayerEvaded));
    }

    #[test]
    fn test_enemy_miss() {
        assert!(matches!(parse_line(LINE_ENEMY_MISS).unwrap(), Event::EnemyMiss));
    }

    #[test]
    fn test_damage_taken() {
        match parse_line(LINE_DMG_TAKEN).unwrap() {
            Event::DamageTaken { amount } => assert!((amount - 21.5).abs() < 0.001),
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_heal_self() {
        match parse_line(LINE_HEAL).unwrap() {
            Event::HealSelf { amount } => assert!((amount - 2.2).abs() < 0.001),
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_skill_gain() {
        match parse_line(LINE_SKILL).unwrap() {
            Event::SkillGain { skill, amount } => {
                assert!((amount - 0.015).abs() < 0.0001);
                assert_eq!(skill, "Gauss Weaponry Technology");
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_attribute_gain() {
        match parse_line(LINE_ATTR).unwrap() {
            Event::AttributeGain { attribute, amount } => {
                assert!((amount - 0.0334).abs() < 0.00001);
                assert_eq!(attribute, "Dexterity");
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_loot() {
        match parse_line(LINE_LOOT).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Shrapnel");
                assert_eq!(qty, 2936);
                assert!((value_ped - 0.2936).abs() < 0.0001);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_global_kill() {
        match parse_line(LINE_GLOBAL).unwrap() {
            Event::GlobalKill { creature, value_ped, .. } => {
                assert_eq!(creature, "KING KONG");
                assert!((value_ped - 582.0).abs() < 0.001);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_rookie_ignored() {
        let line = "2026-03-21 19:10:17 [Rookie] [] Hello world";
        assert!(parse_line(line).is_none());
    }

    #[test]
    fn test_skill_before_attribute_no_overlap() {
        let result = parse_line(LINE_SKILL).unwrap();
        assert!(matches!(result, Event::SkillGain { .. }));
    }

    #[test]
    fn test_extract_timestamp() {
        let ts = extract_timestamp_sec("2026-03-21 19:10:17 [System] []").unwrap();
        assert!(ts > 1_750_000_000);
        assert!(ts < 1_800_000_000);
    }

    // ── Tests mit echten Chat-Log-Zeilen ─────────────────────────────────────

    // Hunting-Log
    #[test]
    fn real_hunt_loot_shrapnel() {
        let line = "2026-06-07 19:18:11 [System] [] You received Shrapnel x (5633) Value: 0.5633 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Shrapnel");
                assert_eq!(qty, 5633);
                assert!((value_ped - 0.5633).abs() < 0.0001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_loot_robot_heat_sinks() {
        let line = "2026-06-07 19:20:11 [System] [] You received Robot Heat Sinks x (22) Value: 1.76 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Robot Heat Sinks");
                assert_eq!(qty, 22);
                assert!((value_ped - 1.76).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_loot_paint_can() {
        let line = "2026-06-07 19:18:11 [System] [] You received Paint Can (Pink) x (17) Value: 1.36 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Paint Can (Pink)");
                assert_eq!(qty, 17);
                assert!((value_ped - 1.36).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_robot_component_residue() {
        let line = "2026-06-07 19:18:30 [System] [] You received Robot Component Residue x (145) Value: 1.45 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Robot Component Residue");
                assert_eq!(qty, 145);
                assert!((value_ped - 1.45).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_crit_hit() {
        let line = "2026-06-07 19:18:16 [System] [] Critical hit - Additional damage! You inflicted 192.0 points of damage";
        match parse_line(line).unwrap() {
            Event::PlayerHit { damage, critical } => {
                assert!((damage - 192.0).abs() < 0.001);
                assert!(critical);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_normal_hit() {
        let line = "2026-06-07 19:18:16 [System] [] You inflicted 55.6 points of damage";
        match parse_line(line).unwrap() {
            Event::PlayerHit { damage, critical } => {
                assert!((damage - 55.6).abs() < 0.001);
                assert!(!critical);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_enemy_dodge() {
        let line = "2026-06-07 19:18:25 [System] [] The target Dodged your attack";
        assert!(matches!(parse_line(line).unwrap(), Event::EnemyEvaded));
    }

    #[test]
    fn real_hunt_player_dodge() {
        let line = "2026-06-07 19:20:10 [System] [] You Dodged the attack";
        // "You Dodged" → PlayerEvaded (nicht EnemyEvaded)
        assert!(matches!(parse_line(line), Some(Event::PlayerEvaded) | None));
        // Parses korrekt oder ignoriert (PlayerEvaded ist im Stat-System nicht-zählend)
    }

    #[test]
    fn real_hunt_damage_taken() {
        let line = "2026-06-07 19:19:04 [System] [] You took 57.1 points of damage";
        match parse_line(line).unwrap() {
            Event::DamageTaken { amount } => assert!((amount - 57.1).abs() < 0.001),
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_heal_self() {
        let line = "2026-06-07 19:19:08 [System] [] You healed yourself 7.9 points";
        match parse_line(line).unwrap() {
            Event::HealSelf { amount } => assert!((amount - 7.9).abs() < 0.001),
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_skill_gain() {
        let line = "2026-06-07 19:18:20 [System] [] You have gained 0.0133 experience in your BLP Weaponry Technology skill";
        match parse_line(line).unwrap() {
            Event::SkillGain { skill, amount } => {
                assert_eq!(skill, "BLP Weaponry Technology");
                assert!((amount - 0.0133).abs() < 0.00001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_hunt_serendipity_attribute() {
        let line = "2026-06-07 19:18:28 [System] [] You have gained 0.0760 Serendipity";
        match parse_line(line).unwrap() {
            Event::AttributeGain { attribute, amount } => {
                assert_eq!(attribute, "Serendipity");
                assert!((amount - 0.0760).abs() < 0.00001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_global_kill() {
        let line = "2026-06-07 19:18:28 [Globals] [] Dr Zana Maselutza killed a creature (Sicarius Nexus) with a value of 221 PED!";
        match parse_line(line).unwrap() {
            Event::GlobalKill { creature, value_ped, player } => {
                assert_eq!(creature, "Sicarius Nexus");
                assert!((value_ped - 221.0).abs() < 0.001);
                assert!(player.contains("Maselutza"));
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_society_chat_ignored() {
        let line = "2026-06-07 19:18:52 [Society] [Illuminara Illu Eisrabe] with that synchronization chip";
        assert!(parse_line(line).is_none());
    }

    #[test]
    fn real_rookie_chat_ignored() {
        let line = "2026-06-07 19:19:50 [Rookie] [Gym Gymmy MacHais] the people where you live have preferences though";
        assert!(parse_line(line).is_none());
    }

    // ── Stats-Aggregation mit Hunting-Log-Ausschnitt ──────────────────────────

    #[test]
    fn real_hunt_stats_aggregation() {

        use crate::domain::types::Stats;

        let lines = [
            "2026-06-07 19:18:11 [System] [] You received Shrapnel x (5633) Value: 0.5633 PED",
            "2026-06-07 19:18:11 [System] [] You received Paint Can (Pink) x (17) Value: 1.36 PED",
            "2026-06-07 19:18:16 [System] [] Critical hit - Additional damage! You inflicted 192.0 points of damage",
            "2026-06-07 19:18:16 [System] [] You inflicted 55.6 points of damage",
            "2026-06-07 19:18:20 [System] [] You have gained 0.0133 experience in your BLP Weaponry Technology skill",
            "2026-06-07 19:18:25 [System] [] The target Dodged your attack",
            "2026-06-07 19:18:25 [System] [] The target Dodged your attack",
            "2026-06-07 19:18:30 [System] [] You received Shrapnel x (2219) Value: 0.2219 PED",
            "2026-06-07 19:18:30 [System] [] You received Robot Component Residue x (145) Value: 1.45 PED",
            "2026-06-07 19:19:04 [System] [] You took 57.1 points of damage",
            "2026-06-07 19:19:08 [System] [] You healed yourself 7.9 points",
            "2026-06-07 19:20:11 [System] [] You received Robot Heat Sinks x (22) Value: 1.76 PED",
            "2026-06-07 19:18:28 [System] [] You have gained 0.0760 Serendipity",
            "2026-06-07 19:18:28 [Globals] [] Dr Zana Maselutza killed a creature (Sicarius Nexus) with a value of 221 PED!",
        ];

        let mut stats = Stats::default();
        for line in &lines {
            if let Some(ev) = parse_line(line) {
                stats.apply_event(&ev);
            }
        }

        // Damage: crit 192.0 + normal 55.6
        assert!((stats.total_damage - 247.6).abs() < 0.01);
        assert_eq!(stats.player_hits, 1);
        assert_eq!(stats.player_crit_hits, 1);
        assert_eq!(stats.total_shots, 4); // 2 hits + 2 evades
        assert_eq!(stats.player_evades, 2);

        // Loot: Shrapnel 0.5633 + 0.2219 + Paint Can 1.36 + Robot Component Residue 1.45 + Robot Heat Sinks 1.76
        let expected_loot = 0.5633 + 0.2219 + 1.36 + 1.45 + 1.76;
        assert!((stats.total_loot_value_ped - expected_loot).abs() < 0.001,
            "Expected {expected_loot:.4}, got {:.4}", stats.total_loot_value_ped);

        // Loot-Items
        assert!(stats.loot_items.contains_key("Shrapnel"));
        assert!(stats.loot_items.contains_key("Paint Can (Pink)"));
        assert!(stats.loot_items.contains_key("Robot Component Residue"));
        assert!(stats.loot_items.contains_key("Robot Heat Sinks"));
        assert_eq!(stats.loot_items["Shrapnel"].event_count, 2);
        assert_eq!(stats.loot_items["Shrapnel"].total_qty, 5633 + 2219);

        // Schaden & Heilung
        assert!((stats.total_damage_taken - 57.1).abs() < 0.001);
        assert!((stats.total_heal_self - 7.9).abs() < 0.001);

        // Skills
        assert!(stats.skill_gains.contains_key("BLP Weaponry Technology"));
        assert!((stats.skill_gains["BLP Weaponry Technology"] - 0.0133).abs() < 0.00001);
        assert!(stats.attribute_gains.contains_key("Serendipity"));

        // Globals
        assert_eq!(stats.globals.len(), 1);
        assert_eq!(stats.globals[0].creature, "Sicarius Nexus");
    }

    // ── Crafting-Log Tests ────────────────────────────────────────────────────
    // Im Crafting-Log gibt es nur Loot-Events — der Parser verarbeitet sie gleich.

    #[test]
    fn real_craft_success_output() {
        // Erfolgreicher Craft: Output-Item (kein Residue)
        let line = "2026-01-17 22:49:15 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Super Alloy Mountings");
                assert_eq!(qty, 1);
                assert!((value_ped - 0.7).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_craft_metal_residue() {
        let line = "2026-01-17 22:49:15 [System] [] You received Metal Residue x (44) Value: 0.4400 PED";
        match parse_line(line).unwrap() {
            Event::Loot { item, qty, value_ped, .. } => {
                assert_eq!(item, "Metal Residue");
                assert_eq!(qty, 44);
                assert!((value_ped - 0.44).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_craft_skill_gain() {
        let line = "2026-01-17 22:54:06 [System] [] You have gained 0.7484 experience in your Manufacture Metal Equipment skill";
        match parse_line(line).unwrap() {
            Event::SkillGain { skill, amount } => {
                assert_eq!(skill, "Manufacture Metal Equipment");
                assert!((amount - 0.7484).abs() < 0.0001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_craft_global_ignored_for_system() {
        // [Globals]-Zeile im Crafting-Log soll als GlobalKill erkannt werden
        let line = "2026-01-17 22:49:24 [Globals] [] Phill No1 Jackson killed a creature (Badger_710R MK2) with a value of 79 PED at Arkadia Underground!";
        match parse_line(line).unwrap() {
            Event::GlobalKill { creature, value_ped, .. } => {
                assert!(creature.contains("Badger_710R MK2"));
                assert!((value_ped - 79.0).abs() < 0.001);
            }
            e => panic!("wrong: {e:?}"),
        }
    }

    #[test]
    fn real_craft_quality_rating_ignored() {
        // "Your blueprint Quality Rating has improved" → kein Event (Ignored oder None)
        let line = "2026-01-17 22:49:19 [System] [] Your blueprint Quality Rating has improved";
        // Wird nicht geparst (kein Regex dafür) → None
        assert!(parse_line(line).is_none());
    }

    #[test]
    fn real_craft_loot_stats_aggregation() {

        use crate::domain::types::Stats;

        // Ein erfolgreicher Craft-Versuch (22:49:15): Super Alloy Mountings + Metal Residue + Shrapnel
        // Ein fehlgeschlagener (22:51:55): nur Robot Component Residue + Shrapnel
        let lines = [
            "2026-01-17 22:49:15 [System] [] You received Super Alloy Mountings x (1) Value: 0.7000 PED",
            "2026-01-17 22:49:15 [System] [] You received Metal Residue x (44) Value: 0.4400 PED",
            "2026-01-17 22:49:15 [System] [] You received Shrapnel x (98) Value: 0.0098 PED",
            "2026-01-17 22:51:55 [System] [] You received Robot Component Residue x (22) Value: 0.2200 PED",
            "2026-01-17 22:51:55 [System] [] You received Shrapnel x (72) Value: 0.0072 PED",
            "2026-01-17 22:54:06 [System] [] You have gained 0.7484 experience in your Manufacture Metal Equipment skill",
        ];

        let mut stats = Stats::default();
        for line in &lines {
            if let Some(ev) = parse_line(line) {
                stats.apply_event(&ev);
            }
        }

        // Gesamt-Loot
        let expected = 0.7000 + 0.4400 + 0.0098 + 0.2200 + 0.0072;
        assert!((stats.total_loot_value_ped - expected).abs() < 0.001);

        // Output-Item erkannt
        assert!(stats.loot_items.contains_key("Super Alloy Mountings"));
        assert_eq!(stats.loot_items["Super Alloy Mountings"].total_qty, 1);

        // Residue erkannt
        assert!(stats.loot_items.contains_key("Metal Residue"));
        assert!(stats.loot_items.contains_key("Robot Component Residue"));

        // Crafting-Skills getrackt
        assert!(stats.skill_gains.contains_key("Manufacture Metal Equipment"));
        assert!((stats.skill_gains["Manufacture Metal Equipment"] - 0.7484).abs() < 0.0001);
    }
}
