use serde::Deserialize;
use crate::domain::types::{CreatureConfig, Maturity};

// ─── DTOs (reine Datencontainer, keine Logik) ────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NexusMobDto {
    pub name: String,
    pub maturities: Vec<NexusMaturityDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NexusMaturityDto {
    pub name: String,
    pub properties: NexusMaturityPropertiesDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NexusMaturityPropertiesDto {
    pub health: Option<f64>,
}

// ─── Adapter: DTO → Domain ────────────────────────────────────────────────────

impl From<NexusMobDto> for CreatureConfig {
    fn from(dto: NexusMobDto) -> Self {
        let mut maturities: Vec<Maturity> = dto.maturities
            .into_iter()
            .filter_map(|m| {
                let hp = m.properties.health?;
                Some(Maturity {
                    name:   m.name,
                    hp_min: hp,
                    hp_max: hp,
                })
            })
            .collect();
        // Nach HP aufsteigend sortieren (Young → Provider → Old Alpha → ...)
        maturities.sort_by(|a, b| a.hp_min.partial_cmp(&b.hp_min).unwrap_or(std::cmp::Ordering::Equal));

        CreatureConfig {
            creature:   dto.name,
            maturities,
        }
    }
}

// ─── API-Fetch ────────────────────────────────────────────────────────────────

/// Lädt eine Kreatur von der Entropianexus-API und gibt sie als Domain-Typ zurück.
pub async fn fetch_creature(mob_name: &str) -> Result<CreatureConfig, String> {
    // URL-Encoding für Leerzeichen und Sonderzeichen im Mob-Namen
    let encoded = js_sys::encode_uri_component(mob_name).as_string()
        .unwrap_or_else(|| mob_name.to_string());
    let url = format!("https://api.entropianexus.com/mobs/{}", encoded);

    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Netzwerkfehler: {:?}", e))?;

    if !response.ok() {
        return Err(format!("API-Fehler: HTTP {}", response.status()));
    }

    let dto: NexusMobDto = response
        .json()
        .await
        .map_err(|e| format!("JSON-Parse-Fehler: {:?}", e))?;

    if dto.maturities.is_empty() {
        return Err("Keine Maturities in der API-Antwort".to_string());
    }

    Ok(dto.into())
}
