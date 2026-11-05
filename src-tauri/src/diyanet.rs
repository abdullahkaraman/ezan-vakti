//! HTTP access to the Diyanet mirror and the timezone lookup.

use crate::schedule::DayTimes;
use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const BASE: &str = "https://ezanvakti.emushaf.net";
const GEOCODER: &str = "https://geocoding-api.open-meteo.com/v1/search";
const TURKIYE_ID: &str = "2";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    RateLimited,
    Status(u16),
    Network,
    Parse,
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            ApiError::RateLimited => {
                "Vakit servisi çok fazla istek aldı. Biraz sonra tekrar dene.".into()
            }
            ApiError::Status(code) => format!("Vakit servisi hata döndü ({code})."),
            ApiError::Network => "Vakit servisine ulaşılamadı. Bağlantını kontrol et.".into(),
            ApiError::Parse => "Vakit servisinden beklenmeyen bir yanıt geldi.".into(),
        }
    }
}

pub fn classify_status(code: u16) -> Result<(), ApiError> {
    match code {
        200..=299 => Ok(()),
        429 => Err(ApiError::RateLimited),
        other => Err(ApiError::Status(other)),
    }
}

/// A country, city or district. The API names the fields differently per level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    #[serde(alias = "UlkeID", alias = "SehirID", alias = "IlceID")]
    pub id: String,
    #[serde(alias = "UlkeAdi", alias = "SehirAdi", alias = "IlceAdi")]
    pub name: String,
    #[serde(alias = "UlkeAdiEn", alias = "SehirAdiEn", alias = "IlceAdiEn")]
    pub name_en: String,
}

pub fn parse_items(body: &str) -> Result<Vec<Item>, ApiError> {
    serde_json::from_str(body).map_err(|_| ApiError::Parse)
}

/// Türkiye is the expected case, so it leads the list instead of KKTC.
pub fn turkiye_first(items: &mut [Item]) {
    items.sort_by_key(|item| item.id != TURKIYE_ID);
}

/// `GreenwichOrtalamaZamani` is deliberately absent: the API answers 3.0 even
/// for Germany and India, whose times are nonetheless local.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawDay {
    miladi_tarih_kisa: String,
    imsak: String,
    gunes: String,
    ogle: String,
    ikindi: String,
    aksam: String,
    yatsi: String,
}

fn parse_day(raw: RawDay) -> Result<DayTimes, ApiError> {
    let date = NaiveDate::parse_from_str(&raw.miladi_tarih_kisa, "%d.%m.%Y")
        .map_err(|_| ApiError::Parse)?;
    let time = |text: &str| NaiveTime::parse_from_str(text, "%H:%M").map_err(|_| ApiError::Parse);
    Ok(DayTimes {
        date,
        times: [
            time(&raw.imsak)?,
            time(&raw.gunes)?,
            time(&raw.ogle)?,
            time(&raw.ikindi)?,
            time(&raw.aksam)?,
            time(&raw.yatsi)?,
        ],
    })
}

pub fn parse_times(body: &str) -> Result<Vec<DayTimes>, ApiError> {
    let raw: Vec<RawDay> = serde_json::from_str(body).map_err(|_| ApiError::Parse)?;
    raw.into_iter().map(parse_day).collect()
}

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<GeoResult>,
}

#[derive(Deserialize)]
struct GeoResult {
    country: Option<String>,
    timezone: Option<String>,
}

/// First result in the right country whose zone is one we can actually use.
pub fn match_timezone(body: &str, country_en: &str) -> Option<String> {
    let wanted = country_en.to_lowercase();
    let response: GeoResponse = serde_json::from_str(body).ok()?;
    response
        .results
        .into_iter()
        .filter(|result| {
            result.country.as_deref().map(str::to_lowercase).as_deref() == Some(&wanted)
        })
        .filter_map(|result| result.timezone)
        .find(|zone| zone.parse::<Tz>().is_ok())
}

pub struct Api {
    client: reqwest::Client,
}

impl Api {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("EzanVakti/0.1")
            .build()
            .expect("HTTP client");
        Self { client }
    }

    async fn get(&self, path: &str) -> Result<String, ApiError> {
        let response = self
            .client
            .get(format!("{BASE}{path}"))
            .send()
            .await
            .map_err(|_| ApiError::Network)?;
        classify_status(response.status().as_u16())?;
        response.text().await.map_err(|_| ApiError::Network)
    }

    pub async fn items(&self, path: &str) -> Result<Vec<Item>, ApiError> {
        parse_items(&self.get(path).await?)
    }

    pub async fn times(&self, district_id: &str) -> Result<Vec<DayTimes>, ApiError> {
        parse_times(&self.get(&format!("/vakitler/{district_id}")).await?)
    }

    async fn geocode(&self, name: &str, country_en: &str) -> Option<String> {
        let response = self
            .client
            .get(GEOCODER)
            .query(&[("name", name), ("count", "10"), ("language", "en")])
            .send()
            .await
            .ok()?;
        match_timezone(&response.text().await.ok()?, country_en)
    }

    /// `None` makes the caller fall back to the Mac's own zone.
    pub async fn find_timezone(
        &self,
        country_id: &str,
        country_en: &str,
        city_en: &str,
        district_en: &str,
    ) -> Option<String> {
        if country_id == TURKIYE_ID {
            return Some("Europe/Istanbul".into());
        }
        match self.geocode(district_en, country_en).await {
            Some(zone) => Some(zone),
            None => self.geocode(city_en, country_en).await,
        }
    }
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISTANBUL: &str = include_str!("../tests/fixtures/vakitler-9541.json");
    const INDIA: &str = include_str!("../tests/fixtures/vakitler-hindistan.json");
    const AALEN: &str = include_str!("../tests/fixtures/geocode-aalen.json");

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    #[test]
    fn parses_real_time_responses() {
        let days = parse_times(ISTANBUL).unwrap();
        assert_eq!(days.len(), 32);
        // The list opens a week in the past, so position says nothing about today.
        assert_eq!(days[0].date, date("2026-10-29"));
        assert_eq!(days[7].date, date("2026-11-05"));
        assert_eq!(days[0].times[0].to_string(), "05:58:00");
        assert_eq!(days[0].times[5].to_string(), "19:32:00");
        assert_eq!(parse_times(INDIA).unwrap().len(), 3);
    }

    #[test]
    fn the_bogus_gmt_field_has_no_influence() {
        let edited = INDIA.replace(
            "\"GreenwichOrtalamaZamani\": 3.0",
            "\"GreenwichOrtalamaZamani\": 9.0",
        );
        let edited = edited.replace(
            "\"GreenwichOrtalamaZamani\":3.0",
            "\"GreenwichOrtalamaZamani\":9.0",
        );
        assert_ne!(edited, INDIA);
        assert_eq!(parse_times(&edited).unwrap(), parse_times(INDIA).unwrap());
    }

    #[test]
    fn parses_every_list_level() {
        let mut countries = parse_items(include_str!("../tests/fixtures/ulkeler.json")).unwrap();
        turkiye_first(&mut countries);
        assert_eq!(countries[0].name, "TÜRKİYE");
        assert_eq!(countries.len(), 202);

        let cities = parse_items(include_str!("../tests/fixtures/sehirler-2.json")).unwrap();
        assert_eq!(cities[0].name, "ADANA");
        assert_eq!(cities[0].id, "500");

        let districts = parse_items(include_str!("../tests/fixtures/ilceler-539.json")).unwrap();
        assert_eq!(districts[0].id, "9535");
        assert_eq!(districts[0].name_en, "ARNAVUTKOY");
    }

    #[test]
    fn items_round_trip_through_the_disk_format() {
        let items = parse_items(include_str!("../tests/fixtures/sehirler-2.json")).unwrap();
        let stored = serde_json::to_string(&items).unwrap();
        assert_eq!(parse_items(&stored).unwrap(), items);
    }

    #[test]
    fn timezone_matches_only_within_the_country() {
        assert_eq!(match_timezone(AALEN, "GERMANY").unwrap(), "Europe/Berlin");
        assert_eq!(match_timezone(AALEN, "NORWAY").unwrap(), "Europe/Oslo");
        assert_eq!(match_timezone(AALEN, "JAPAN"), None);
    }

    #[test]
    fn timezone_lookup_survives_empty_or_broken_answers() {
        assert_eq!(
            match_timezone(r#"{"generationtime_ms":0.1}"#, "GERMANY"),
            None
        );
        assert_eq!(match_timezone("<html>", "GERMANY"), None);
        let unusable = r#"{"results":[{"country":"Germany","timezone":"Mars/Base"}]}"#;
        assert_eq!(match_timezone(unusable, "GERMANY"), None);
    }

    #[test]
    fn status_codes_are_classified() {
        assert_eq!(classify_status(200), Ok(()));
        assert_eq!(classify_status(429), Err(ApiError::RateLimited));
        assert_eq!(classify_status(503), Err(ApiError::Status(503)));
        assert!(ApiError::RateLimited.message().contains("çok fazla istek"));
    }

    /// Hits the real services: `cargo test -- --ignored`.
    #[test]
    #[ignore = "ağ gerektirir"]
    fn live_services_answer_in_the_expected_shape() {
        tauri::async_runtime::block_on(async {
            let api = Api::new();
            assert!(api.items("/ulkeler").await.unwrap().len() > 100);
            assert!(api.times("9541").await.unwrap().len() >= 7);
            let zone = api.find_timezone("13", "GERMANY", "BADEN WURTTEMBERG", "AALEN");
            assert_eq!(zone.await.as_deref(), Some("Europe/Berlin"));
            let zone = api.find_timezone("2", "TÜRKİYE", "ISTANBUL", "ISTANBUL");
            assert_eq!(zone.await.as_deref(), Some("Europe/Istanbul"));
        });
    }

    #[test]
    fn malformed_time_responses_are_errors() {
        assert_eq!(parse_times("nope"), Err(ApiError::Parse));
        let bad = ISTANBUL.replacen("\"Imsak\":\"", "\"Imsak\":\"x", 1);
        let bad = bad.replacen("\"Imsak\": \"", "\"Imsak\": \"x", 1);
        assert_eq!(parse_times(&bad), Err(ApiError::Parse));
    }
}
