use std::str::FromStr;
use std::time::Duration;

use jiff::Zoned;
use jiff::civil::{Date, Time, Weekday};
use serde::Deserialize;
use thiserror::Error;

pub const ZONE: &str = "America/Toronto";

pub fn now() -> Result<Zoned, jiff::Error> {
    jiff::Timestamp::now().in_tz(ZONE)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct Weekly {
    pub weekday: Weekday,
    pub time: Time,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("horaire {0:?} : attendu un jour (mon à sun) et une heure, ex. \"sun 12:00\"")]
pub struct BadWeekly(String);

impl FromStr for Weekly {
    type Err = BadWeekly;

    fn from_str(text: &str) -> Result<Self, BadWeekly> {
        let bad = || BadWeekly(text.to_string());
        let (day, time) = text.split_once(' ').ok_or_else(bad)?;
        let weekday = match day {
            "mon" => Weekday::Monday,
            "tue" => Weekday::Tuesday,
            "wed" => Weekday::Wednesday,
            "thu" => Weekday::Thursday,
            "fri" => Weekday::Friday,
            "sat" => Weekday::Saturday,
            "sun" => Weekday::Sunday,
            _ => return Err(bad()),
        };
        let (hour, minute) = time.split_once(':').ok_or_else(bad)?;
        let number = |part: &str| part.parse::<i8>().map_err(|_| bad());
        let time = Time::new(number(hour)?, number(minute)?, 0, 0).map_err(|_| bad())?;
        Ok(Self { weekday, time })
    }
}

impl TryFrom<String> for Weekly {
    type Error = BadWeekly;

    fn try_from(text: String) -> Result<Self, BadWeekly> {
        text.parse()
    }
}

pub struct Calendar {
    weekly: Weekly,
    next: Option<Zoned>,
}

impl Calendar {
    pub fn new(weekly: Weekly, now: &Zoned) -> Self {
        Self {
            weekly,
            next: weekly.next(now).ok(),
        }
    }

    pub fn due_at(&mut self, now: &Zoned) -> Option<Date> {
        let slot = self.next.take_if(|slot| *slot <= *now)?;
        self.next = self.weekly.next(now).ok();
        Some(slot.date())
    }
}

impl Weekly {
    pub fn next(&self, now: &Zoned) -> Result<Zoned, jiff::Error> {
        let day = self.closest(now.date(), 1)?;
        let slot = self.at(day, now)?;
        if slot > *now {
            Ok(slot)
        } else {
            self.at(day.nth_weekday(1, self.weekday)?, now)
        }
    }

    pub fn previous(&self, now: &Zoned) -> Result<Zoned, jiff::Error> {
        let day = self.closest(now.date(), -1)?;
        let slot = self.at(day, now)?;
        if slot <= *now {
            Ok(slot)
        } else {
            self.at(day.nth_weekday(-1, self.weekday)?, now)
        }
    }

    pub fn missed(&self, now: &Zoned, catch_up: Duration) -> Option<Zoned> {
        self.previous(now)
            .ok()
            .filter(|slot| now.duration_since(slot).unsigned_abs() < catch_up)
    }

    fn closest(&self, date: Date, step: i32) -> Result<Date, jiff::Error> {
        if date.weekday() == self.weekday {
            Ok(date)
        } else {
            date.nth_weekday(step, self.weekday)
        }
    }

    fn at(&self, date: Date, now: &Zoned) -> Result<Zoned, jiff::Error> {
        date.to_datetime(self.time).to_zoned(now.time_zone().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Zoned {
        format!("{text}[{ZONE}]").parse().unwrap()
    }

    fn sunday_noon() -> Weekly {
        "sun 12:00".parse().unwrap()
    }

    #[test]
    fn parses_a_day_and_a_time() {
        assert_eq!(
            "sun 12:00".parse(),
            Ok(Weekly {
                weekday: Weekday::Sunday,
                time: Time::constant(12, 0, 0, 0)
            })
        );
        assert_eq!("mon 9:30".parse::<Weekly>().map(|w| w.weekday), Ok(Weekday::Monday));
        for bad in ["dim 12:00", "sun 25:00", "sun", "", "sun 12:00 extra"] {
            assert!(bad.parse::<Weekly>().is_err(), "{bad}");
        }
    }

    #[test]
    fn next_is_the_coming_slot() {
        let w = sunday_noon();
        let next = |now| w.next(&at(now)).unwrap();
        assert_eq!(next("2026-11-08T11:00"), at("2026-11-08T12:00"));
        assert_eq!(next("2026-11-08T12:00"), at("2026-11-15T12:00"));
        assert_eq!(next("2026-11-07T20:00"), at("2026-11-08T12:00"));
        assert_eq!(next("2026-11-09T08:00"), at("2026-11-15T12:00"));
    }

    #[test]
    fn previous_is_the_last_slot() {
        let w = sunday_noon();
        let previous = |now| w.previous(&at(now)).unwrap();
        assert_eq!(previous("2026-11-08T12:00"), at("2026-11-08T12:00"));
        assert_eq!(previous("2026-11-08T11:59"), at("2026-11-01T12:00"));
        assert_eq!(previous("2026-11-11T08:00"), at("2026-11-08T12:00"));
    }

    #[test]
    fn noon_stays_noon_across_daylight_saving() {
        let w = sunday_noon();
        let fall = w.next(&at("2026-10-31T13:00")).unwrap();
        assert_eq!(
            (fall.time(), fall.offset().seconds()),
            (Time::constant(12, 0, 0, 0), -5 * 3600)
        );
        let spring = w.next(&at("2027-03-13T20:00")).unwrap();
        assert_eq!(
            (spring.time(), spring.offset().seconds()),
            (Time::constant(12, 0, 0, 0), -4 * 3600)
        );
    }

    #[test]
    fn the_calendar_fires_once_per_slot() {
        let mut calendar = Calendar::new(sunday_noon(), &at("2026-11-08T11:00"));
        assert_eq!(calendar.due_at(&at("2026-11-08T11:59")), None);
        assert_eq!(
            calendar.due_at(&at("2026-11-08T12:00")),
            Some(Date::constant(2026, 11, 8))
        );
        assert_eq!(calendar.due_at(&at("2026-11-08T12:05")), None);
        assert_eq!(
            calendar.due_at(&at("2026-11-29T12:30")),
            Some(Date::constant(2026, 11, 15))
        );
        assert_eq!(calendar.due_at(&at("2026-11-29T12:31")), None);
    }

    #[test]
    fn a_recent_slot_is_caught_up() {
        let w = sunday_noon();
        let two_hours = Duration::from_hours(2);
        assert_eq!(
            w.missed(&at("2026-11-08T13:30"), two_hours),
            Some(at("2026-11-08T12:00"))
        );
        assert_eq!(w.missed(&at("2026-11-08T14:30"), two_hours), None);
        assert_eq!(w.missed(&at("2026-11-08T11:00"), two_hours), None);
    }
}
