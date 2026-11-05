//! JSON persistence in the app data directory. Anything unreadable counts as
//! absent: a corrupt cache must never keep the app from starting.

use crate::diyanet::Item;
use crate::schedule::{DayTimes, Reminders};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub country_id: String,
    pub country_en: String,
    pub city: String,
    pub city_en: String,
    pub district_id: String,
    pub district: String,
    pub district_en: String,
    /// IANA name; `None` when the lookup failed and the Mac's zone is in use.
    pub timezone: Option<String>,
}

/// Location and times live together so one can never outlive the other.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub location: Option<Location>,
    pub days: Vec<DayTimes>,
    /// Absent in files written by v0.1.0.
    #[serde(default)]
    pub reminders: Reminders,
}

#[derive(Serialize, Deserialize)]
pub struct CachedList {
    pub fetched_at: DateTime<Utc>,
    pub items: Vec<Item>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn saved_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    fn list_path(&self, key: &str) -> PathBuf {
        self.dir.join("lists").join(format!("{key}.json"))
    }

    pub fn load(&self) -> Saved {
        read(&self.saved_path()).unwrap_or_default()
    }

    pub fn save(&self, saved: &Saved) {
        write(&self.saved_path(), saved);
    }

    pub fn load_list(&self, key: &str) -> Option<CachedList> {
        read(&self.list_path(key))
    }

    pub fn save_list(&self, key: &str, list: &CachedList) {
        write(&self.list_path(key), list);
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Best effort: a failed write costs a later re-fetch, never a crash.
fn write<T: Serialize>(path: &Path, value: &T) {
    let attempt = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec(value)?)?;
        fs::rename(&temporary, path)
    };
    if let Err(error) = attempt() {
        eprintln!("kayıt yazılamadı: {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diyanet::parse_times;

    fn temporary(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ezan-vakti-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn location() -> Location {
        Location {
            country_id: "2".into(),
            country_en: "TÜRKİYE".into(),
            city: "İSTANBUL".into(),
            city_en: "ISTANBUL".into(),
            district_id: "9541".into(),
            district: "KADIKÖY".into(),
            district_en: "KADIKOY".into(),
            timezone: Some("Europe/Istanbul".into()),
        }
    }

    #[test]
    fn saved_state_round_trips() {
        let dir = temporary("roundtrip");
        let store = Store::new(dir.clone());
        let saved = Saved {
            location: Some(location()),
            days: parse_times(include_str!("../tests/fixtures/vakitler-9541.json")).unwrap(),
            reminders: [(crate::schedule::Vakit::Ikindi, 15)].into_iter().collect(),
        };
        store.save(&saved);
        assert_eq!(store.load(), saved);
        assert!(!dir.join("state.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn state_written_by_the_previous_version_still_loads() {
        let dir = temporary("upgrade");
        fs::create_dir_all(&dir).unwrap();
        let old = serde_json::json!({ "location": location(), "days": [] });
        fs::write(dir.join("state.json"), old.to_string()).unwrap();
        let saved = Store::new(dir.clone()).load();
        assert_eq!(saved.location, Some(location()));
        assert!(saved.reminders.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_or_corrupt_files_read_as_empty() {
        let dir = temporary("corrupt");
        let store = Store::new(dir.clone());
        assert_eq!(store.load(), Saved::default());

        fs::create_dir_all(dir.join("lists")).unwrap();
        fs::write(dir.join("state.json"), b"{not json").unwrap();
        fs::write(dir.join("lists/countries.json"), b"\xff\xfe").unwrap();
        assert_eq!(store.load(), Saved::default());
        assert!(store.load_list("countries").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lists_are_stored_per_key() {
        let dir = temporary("lists");
        let store = Store::new(dir.clone());
        let list = CachedList {
            fetched_at: Utc::now(),
            items: vec![Item {
                id: "2".into(),
                name: "TÜRKİYE".into(),
                name_en: "TÜRKİYE".into(),
            }],
        };
        store.save_list("countries", &list);
        assert_eq!(store.load_list("countries").unwrap().items, list.items);
        assert!(store.load_list("cities-2").is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
