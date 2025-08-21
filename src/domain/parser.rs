use once_cell::sync::Lazy;
use regex::Regex;
use super::types::Event;

// Regexe einmal kompilieren (schnell zur Laufzeit)
static RE_LOOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} \[System\] \[\] You received (?P<item>.+?) x \((?P<qty>\d+)\) Value: (?P<val>[0-9]*\.?[0-9]+) PED"#).unwrap()
});
static RE_CRIT_AND_INFLICT: Lazy<Regex> = Lazy::new(|| {
    // Variante: "Critical hit - Additional damage! You inflicted 187.7 points of damage"
    Regex::new(r#"\[System\] \[\] (?:Critical hit - Additional damage!\s*)?You inflicted (?P<dmg>[0-9]*\.?[0-9]+) points of damage"#).unwrap()
});
static RE_ENEMY_MISS: Lazy<Regex> = Lazy::new(|| {
    // "The attack missed you"
    Regex::new(r#"\[System\] \[\] The attack missed you"#).unwrap()
});
static RE_ENEMY_EVADED: Lazy<Regex> = Lazy::new(|| {
    // "The target Dodged your attack" (und evtl. "Evaded")
    Regex::new(r#"\[System\] \[\] The target (Dodged|Evaded) your attack"#).unwrap()
});
static RE_PLAYER_MISS: Lazy<Regex> = Lazy::new(|| {
    // Falls es Lines wie "You missed" o.ä. gibt; sonst bleibt ohne Effekt
    Regex::new(r#"\[System\] \[\] You (missed|miss)($| )"#).unwrap()
});

static RE_PLAYER_EVADED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[System\] \[\] (?:You (Dodged|Evaded) the attack)"#).unwrap()
});


pub fn parse_line(line: &str) -> Option<Event> {
    // nur [System] ist relevant
    if !line.contains("[System]") {
        return None;
    }

    // Loot immer zuerst prüfen (damit "received" nicht von anderen Regex in die Quere kommt)
    if let Some(c) = RE_LOOT.captures(line) {
        let item = c.name("item")?.as_str().trim().to_string();
        let qty: u32 = c.name("qty")?.as_str().parse().ok()?;
        let value_ped: f32 = c.name("val")?.as_str().parse().ok()?;
        return Some(Event::Loot { item, qty, value_ped });
    }

    // Treffer (kritisch oder normal)
    if let Some(c) = RE_CRIT_AND_INFLICT.captures(line) {
        let dmg: f64 = c.name("dmg")?.as_str().parse().ok()?;
        // Kritische Treffer sind erkennbar, wenn "Critical hit" im Text steht
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

    // optional: eigener Miss (nur falls dein Log solche Zeilen hat)
    if RE_PLAYER_MISS.is_match(line) {
        return Some(Event::PlayerMiss);
    }

    if RE_PLAYER_EVADED.is_match(line) {
        return Some(Event::PlayerEvaded);
    }

    // alles andere ignorieren (Skills, Chat etc.)
    None
}
