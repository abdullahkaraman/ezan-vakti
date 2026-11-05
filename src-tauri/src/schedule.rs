//! Pure prayer-time logic: no I/O, and `now` is always a parameter.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prayer {
    Imsak,
    Gunes,
    Ogle,
    Ikindi,
    Aksam,
    Yatsi,
}

impl Prayer {
    pub const ALL: [Prayer; 6] = [
        Prayer::Imsak,
        Prayer::Gunes,
        Prayer::Ogle,
        Prayer::Ikindi,
        Prayer::Aksam,
        Prayer::Yatsi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Prayer::Imsak => "İmsak",
            Prayer::Gunes => "Güneş",
            Prayer::Ogle => "Öğle",
            Prayer::Ikindi => "İkindi",
            Prayer::Aksam => "Akşam",
            Prayer::Yatsi => "Yatsı",
        }
    }

    /// The name with its dative suffix, as used in "İkindi'ye 43 dk".
    pub fn dative(self) -> &'static str {
        match self {
            Prayer::Imsak => "İmsak'a",
            Prayer::Gunes => "Güneş'e",
            Prayer::Ogle => "Öğle'ye",
            Prayer::Ikindi => "İkindi'ye",
            Prayer::Aksam => "Akşam'a",
            Prayer::Yatsi => "Yatsı'ya",
        }
    }
}

/// One day's published wall-clock times, in `Prayer::ALL` order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayTimes {
    pub date: NaiveDate,
    pub times: [NaiveTime; 6],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub prayer: Prayer,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// `None` only when `now` precedes the first known boundary.
    pub current: Option<Prayer>,
    pub next: Boundary,
    pub remaining_secs: i64,
}

fn resolve(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(at) => at.with_timezone(&Utc),
        // Autumn DST overlap: the first occurrence is the one that comes up next.
        LocalResult::Ambiguous(first, _) => first.with_timezone(&Utc),
        // Spring DST gap: this wall-clock time does not exist, so step past it.
        LocalResult::None => resolve(tz, local + Duration::hours(1)),
    }
}

/// Flattens the days into one instant-ordered list.
///
/// At high latitudes a published time can fall after midnight of the following
/// day (Yatsı at 00:30), which shows up as a time earlier than its predecessor.
pub fn boundaries(days: &[DayTimes], tz: Tz) -> Vec<Boundary> {
    let mut sorted: Vec<&DayTimes> = days.iter().collect();
    sorted.sort_by_key(|day| day.date);

    let mut list = Vec::with_capacity(sorted.len() * 6);
    for day in sorted {
        let mut previous: Option<NaiveDateTime> = None;
        for (prayer, time) in Prayer::ALL.into_iter().zip(day.times) {
            let mut local = day.date.and_time(time);
            if previous.is_some_and(|before| local < before) {
                local += Duration::days(1);
            }
            previous = Some(local);
            list.push(Boundary {
                prayer,
                at: resolve(tz, local),
            });
        }
    }
    list.sort_by_key(|boundary| boundary.at);
    list
}

/// `None` means the data has run out: showing nothing beats showing a wrong time.
pub fn status(list: &[Boundary], now: DateTime<Utc>) -> Option<Status> {
    let index = list.partition_point(|boundary| boundary.at <= now);
    let next = *list.get(index)?;
    let millis = (next.at - now).num_milliseconds();
    Some(Status {
        current: index.checked_sub(1).map(|i| list[i].prayer),
        next,
        remaining_secs: (millis + 999) / 1000,
    })
}

/// Rounds up, so "1 dk" stays on screen until the time actually arrives.
pub fn format_remaining(secs: i64) -> String {
    let minutes = (secs + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} dk"),
        (h, 0) => format!("{h} sa"),
        (h, m) => format!("{h} sa {m} dk"),
    }
}

pub fn title(status: &Status) -> String {
    format!(
        "{} {}",
        status.next.prayer.dative(),
        format_remaining(status.remaining_secs)
    )
}

/// At most two characters, for the Windows tray icon: "43", "2s", "9+".
pub fn tray_label(secs: i64) -> String {
    let minutes = (secs + 59) / 60;
    match minutes {
        0..=99 => minutes.to_string(),
        100..=599 => format!("{}s", minutes / 60),
        _ => "9+".to_string(),
    }
}

pub fn today(days: &[DayTimes], tz: Tz, now: DateTime<Utc>) -> Option<&DayTimes> {
    let date = now.with_timezone(&tz).date_naive();
    days.iter().find(|day| day.date == date)
}

/// Days of data left after today; negative when even today is missing.
pub fn days_left(days: &[DayTimes], tz: Tz, now: DateTime<Utc>) -> i64 {
    let date = now.with_timezone(&tz).date_naive();
    days.iter()
        .map(|day| (day.date - date).num_days())
        .max()
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::{Asia::Kolkata, Europe::Berlin, Europe::Istanbul};

    fn day(date: &str, times: [&str; 6]) -> DayTimes {
        DayTimes {
            date: date.parse().unwrap(),
            times: times.map(|t| NaiveTime::parse_from_str(t, "%H:%M").unwrap()),
        }
    }

    fn sample(date: &str) -> DayTimes {
        day(date, ["06:00", "07:30", "12:50", "15:30", "18:00", "19:20"])
    }

    fn at(tz: Tz, local: &str) -> DateTime<Utc> {
        let naive = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M:%S").unwrap();
        tz.from_local_datetime(&naive)
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn every_interval_counts_down_to_the_next_boundary() {
        let days = [sample("2026-11-05"), sample("2026-11-06")];
        let list = boundaries(&days, Istanbul);
        let cases = [
            ("05:00:00", None, Prayer::Imsak),
            ("06:10:00", Some(Prayer::Imsak), Prayer::Gunes),
            ("09:00:00", Some(Prayer::Gunes), Prayer::Ogle),
            ("13:00:00", Some(Prayer::Ogle), Prayer::Ikindi),
            ("16:00:00", Some(Prayer::Ikindi), Prayer::Aksam),
            ("18:30:00", Some(Prayer::Aksam), Prayer::Yatsi),
        ];
        for (time, current, next) in cases {
            let now = at(Istanbul, &format!("2026-11-05 {time}"));
            let status = status(&list, now).unwrap();
            assert_eq!(status.current, current, "{time}");
            assert_eq!(status.next.prayer, next, "{time}");
        }
    }

    #[test]
    fn after_yatsi_counts_to_tomorrows_imsak_across_midnight() {
        let days = [sample("2026-11-05"), sample("2026-11-06")];
        let list = boundaries(&days, Istanbul);
        for local in ["2026-11-05 23:00:00", "2026-11-06 01:00:00"] {
            let status = status(&list, at(Istanbul, local)).unwrap();
            assert_eq!(status.current, Some(Prayer::Yatsi));
            assert_eq!(status.next.at, at(Istanbul, "2026-11-06 06:00:00"));
        }
    }

    #[test]
    fn the_exact_boundary_moves_on_to_the_next_one() {
        let list = boundaries(&[sample("2026-11-05")], Istanbul);
        let status = status(&list, at(Istanbul, "2026-11-05 12:50:00")).unwrap();
        assert_eq!(status.current, Some(Prayer::Ogle));
        assert_eq!(status.next.prayer, Prayer::Ikindi);
    }

    #[test]
    fn remaining_time_rounds_up_to_the_minute() {
        assert_eq!(format_remaining(1), "1 dk");
        assert_eq!(format_remaining(60), "1 dk");
        assert_eq!(format_remaining(61), "2 dk");
        assert_eq!(format_remaining(3540), "59 dk");
        assert_eq!(format_remaining(3541), "1 sa");
        assert_eq!(format_remaining(3600), "1 sa");
        assert_eq!(format_remaining(3660), "1 sa 1 dk");
        assert_eq!(format_remaining(3661), "1 sa 2 dk");
        assert_eq!(format_remaining(7200), "2 sa");
    }

    #[test]
    fn tray_labels_stay_two_characters() {
        assert_eq!(tray_label(1), "1");
        assert_eq!(tray_label(99 * 60), "99");
        assert_eq!(tray_label(100 * 60), "1s");
        assert_eq!(tray_label(599 * 60), "9s");
        assert_eq!(tray_label(600 * 60), "9+");
        for minutes in 1..1500 {
            assert!(tray_label(minutes * 60).chars().count() <= 2);
        }
    }

    #[test]
    fn sub_second_remainders_do_not_round_down_to_zero() {
        let list = boundaries(&[sample("2026-11-05")], Istanbul);
        let now = at(Istanbul, "2026-11-05 12:49:59") + Duration::milliseconds(500);
        let status = status(&list, now).unwrap();
        assert_eq!(status.remaining_secs, 1);
        assert_eq!(title(&status), "Öğle'ye 1 dk");
    }

    #[test]
    fn title_uses_the_dative_form() {
        let list = boundaries(&[sample("2026-11-05")], Istanbul);
        let status = status(&list, at(Istanbul, "2026-11-05 14:47:00")).unwrap();
        assert_eq!(title(&status), "İkindi'ye 43 dk");
    }

    #[test]
    fn yatsi_past_midnight_stays_after_aksam() {
        let days = [day(
            "2026-06-20",
            ["02:10", "04:00", "13:10", "17:30", "22:40", "00:30"],
        )];
        let list = boundaries(&days, Berlin);
        assert_eq!(list[5].prayer, Prayer::Yatsi);
        assert_eq!(list[5].at, at(Berlin, "2026-06-21 00:30:00"));
        let status = status(&list, at(Berlin, "2026-06-20 23:00:00")).unwrap();
        assert_eq!(status.next.prayer, Prayer::Yatsi);
    }

    #[test]
    fn equal_consecutive_times_are_not_pushed_a_day_ahead() {
        let days = [day(
            "2026-06-20",
            ["03:00", "03:00", "13:10", "17:30", "21:00", "22:00"],
        )];
        let list = boundaries(&days, Berlin);
        assert_eq!(list[0].at, list[1].at);
    }

    #[test]
    fn times_are_read_in_the_location_zone_not_the_system_zone() {
        let days = [sample("2026-11-05")];
        assert_eq!(
            boundaries(&days, Berlin)[2].at,
            at(Berlin, "2026-11-05 12:50:00")
        );
        assert_eq!(
            boundaries(&days, Kolkata)[2].at,
            "2026-11-05T07:20:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    #[test]
    fn european_dst_change_shifts_the_utc_instant() {
        let days = [sample("2026-10-24"), sample("2026-10-25")];
        let list = boundaries(&days, Berlin);
        assert_eq!(list[0].at.to_rfc3339(), "2026-10-24T04:00:00+00:00");
        assert_eq!(list[6].at.to_rfc3339(), "2026-10-25T05:00:00+00:00");
    }

    #[test]
    fn dst_overlap_and_gap_still_resolve() {
        let overlap = day(
            "2026-10-25",
            ["02:30", "07:30", "12:50", "15:30", "18:00", "19:20"],
        );
        assert_eq!(
            boundaries(&[overlap], Berlin)[0].at.to_rfc3339(),
            "2026-10-25T00:30:00+00:00"
        );
        let gap = day(
            "2027-03-28",
            ["02:30", "07:30", "12:50", "15:30", "18:00", "19:20"],
        );
        assert_eq!(
            boundaries(&[gap], Berlin)[0].at.to_rfc3339(),
            "2027-03-28T01:30:00+00:00"
        );
    }

    #[test]
    fn running_out_of_data_yields_no_status() {
        let list = boundaries(&[sample("2026-11-05")], Istanbul);
        assert!(status(&list, at(Istanbul, "2026-11-05 20:00:00")).is_none());
        assert!(status(&[], at(Istanbul, "2026-11-05 20:00:00")).is_none());
    }

    #[test]
    fn today_and_days_left_use_the_location_date() {
        let days = [sample("2026-11-05"), sample("2026-11-06")];
        // Still the 5th in UTC, already the 6th in Kolkata.
        let now = "2026-11-05T19:00:00Z".parse().unwrap();
        assert_eq!(today(&days, Kolkata, now).unwrap().date, days[1].date);
        assert_eq!(today(&days, Berlin, now).unwrap().date, days[0].date);
        assert_eq!(days_left(&days, Kolkata, now), 0);
        assert_eq!(days_left(&days, Berlin, now), 1);
        assert_eq!(days_left(&[], Berlin, now), -1);
    }
}
