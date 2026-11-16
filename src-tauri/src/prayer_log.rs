//! The answers to "did you pray?", appended one JSON object per line to
//! `prayer-log.jsonl` for later analysis. Nothing else reads them.

use crate::schedule::Vakit;
use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const FILE: &str = "prayer-log.jsonl";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Kildim,
    HenuzDegil,
    /// Closed from the title bar without answering.
    Kapatildi,
    /// A newer question replaced this one first.
    Cevapsiz,
}

impl Answer {
    /// Only what the buttons send; the other two are decided by the backend.
    pub fn from_button(text: &str) -> Option<Self> {
        match text {
            "kildim" => Some(Self::Kildim),
            "henuz_degil" => Some(Self::HenuzDegil),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub vakit: Vakit,
    /// The prayer day the question is about.
    pub date: NaiveDate,
    pub minutes_left: i64,
    pub asked_at: DateTime<Local>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Record {
    date: String,
    vakit: &'static str,
    minutes_left: i64,
    asked_at: String,
    answered_at: String,
    answer: Answer,
}

impl Record {
    pub fn new(pending: &Pending, answer: Answer, answered_at: DateTime<Local>) -> Self {
        Self {
            date: pending.date.format("%Y-%m-%d").to_string(),
            vakit: pending.vakit.key(),
            minutes_left: pending.minutes_left,
            asked_at: pending.asked_at.to_rfc3339(),
            answered_at: answered_at.to_rfc3339(),
            answer,
        }
    }

    pub fn line(&self) -> String {
        serde_json::to_string(self).expect("a record is always serializable")
    }
}

/// Puts `next` in the slot; a question still waiting there was never answered.
pub fn replace(slot: &mut Option<Pending>, next: Pending, now: DateTime<Local>) -> Option<Record> {
    slot.replace(next)
        .map(|old| Record::new(&old, Answer::Cevapsiz, now))
}

/// Takes the waiting question, so whoever answers first records it and the
/// close that follows a button has nothing left to record.
pub fn resolve(slot: &mut Option<Pending>, answer: Answer, now: DateTime<Local>) -> Option<Record> {
    slot.take().map(|old| Record::new(&old, answer, now))
}

/// Append only. A failure is returned, never fatal: the log must not get in the
/// way of the window closing.
pub fn append(dir: &Path, record: &Record) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE))?;
    writeln!(file, "{}", record.line())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn pending(minutes_left: i64) -> Pending {
        Pending {
            vakit: Vakit::Ogle,
            date: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            minutes_left,
            asked_at: Local.with_ymd_and_hms(2026, 11, 5, 15, 20, 0).unwrap(),
        }
    }

    #[test]
    fn a_record_is_one_json_line_with_stable_keys() {
        let answered = pending(20).asked_at + Duration::seconds(30);
        let record = Record::new(&pending(20), Answer::HenuzDegil, answered);
        let line = record.line();
        assert!(!line.contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["date"], "2026-11-05");
        assert_eq!(value["vakit"], "ogle");
        assert_eq!(value["minutes_left"], 20);
        assert_eq!(value["answer"], "henuz_degil");
        assert_eq!(value["asked_at"], pending(20).asked_at.to_rfc3339());
        assert_eq!(value["answered_at"], answered.to_rfc3339());
        assert_eq!(value.as_object().unwrap().len(), 6);
    }

    #[test]
    fn answers_serialize_to_their_documented_names() {
        let names: Vec<String> = [
            Answer::Kildim,
            Answer::HenuzDegil,
            Answer::Kapatildi,
            Answer::Cevapsiz,
        ]
        .iter()
        .map(|a| serde_json::to_string(a).unwrap())
        .collect();
        assert_eq!(
            names,
            [
                "\"kildim\"",
                "\"henuz_degil\"",
                "\"kapatildi\"",
                "\"cevapsiz\""
            ]
        );
    }

    #[test]
    fn buttons_may_only_send_the_two_answers() {
        assert_eq!(Answer::from_button("kildim"), Some(Answer::Kildim));
        assert_eq!(Answer::from_button("henuz_degil"), Some(Answer::HenuzDegil));
        for other in ["kapatildi", "cevapsiz", "", "Kildim", "evet"] {
            assert_eq!(Answer::from_button(other), None);
        }
    }

    #[test]
    fn replacing_an_unanswered_question_records_it_as_cevapsiz() {
        let mut slot = None;
        let now = pending(20).asked_at;
        assert!(replace(&mut slot, pending(20), now).is_none());
        let record = replace(&mut slot, pending(5), now).unwrap();
        assert_eq!(record.answer, Answer::Cevapsiz);
        assert_eq!(record.minutes_left, 20);
        assert_eq!(slot.unwrap().minutes_left, 5);
    }

    #[test]
    fn a_question_is_recorded_once_whoever_answers_first() {
        let mut slot = Some(pending(20));
        let now = pending(20).asked_at;
        assert_eq!(
            resolve(&mut slot, Answer::Kildim, now).unwrap().answer,
            Answer::Kildim
        );
        assert!(resolve(&mut slot, Answer::Kapatildi, now).is_none());
    }

    #[test]
    fn append_adds_lines_without_rewriting() {
        let dir = std::env::temp_dir().join(format!("ezan-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let now = pending(20).asked_at;
        append(&dir, &Record::new(&pending(20), Answer::Kildim, now)).unwrap();
        append(&dir, &Record::new(&pending(5), Answer::Kapatildi, now)).unwrap();
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with('\n'));
    }
}
