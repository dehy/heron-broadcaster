// SPDX-License-Identifier: AGPL-3.0-or-later

//! Schedules: which clock runs at each hour of the week, and the periods a
//! schedule other than the default one plays (`docs/rotation.md#scheduling`).
//!
//! - The default schedule plays outside every period.
//! - When periods overlap, the one that started last plays.
//! - Times are station time; periods start and end on the hour, the end
//!   excluded.

use std::collections::HashMap;

use jiff::civil::{Date, DateTime};
use serde::{Deserialize, Serialize};

use crate::ClockId;

pub type ScheduleId = u64;

/// A week of hours painted with clocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schedule {
    pub id: ScheduleId,
    pub name: String,
    /// ISO day of the week (1 = Monday … 7 = Sunday) and hour (0 to 23).
    pub hours: HashMap<(u8, u8), ClockId>,
    /// When it plays; always empty for the default schedule.
    pub periods: Vec<Period>,
}

/// When a schedule other than the default one plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Period {
    /// Station time, on the hour.
    #[cfg_attr(feature = "openapi", schema(value_type = String, example = "2026-12-01T00:00:00"))]
    pub start: DateTime,
    /// Station time, on the hour, excluded.
    #[cfg_attr(feature = "openapi", schema(value_type = String, example = "2027-01-01T00:00:00"))]
    pub end: DateTime,
    /// Every year on these days and hours (the years are ignored); the
    /// range may run over the new year.
    #[serde(default)]
    pub every_year: bool,
}

/// Month, day and time: a date in any year.
type InYear = (i8, i8, i8, i8, i8, i32);

fn in_year(at: DateTime) -> InYear {
    (
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.subsec_nanosecond(),
    )
}

fn on_the_hour(at: DateTime) -> bool {
    at.minute() == 0 && at.second() == 0 && at.subsec_nanosecond() == 0
}

impl Period {
    pub fn check(&self) -> Result<(), String> {
        if !on_the_hour(self.start) || !on_the_hour(self.end) {
            return Err("periods start and end on the hour".to_owned());
        }
        if self.every_year {
            if in_year(self.start) == in_year(self.end) {
                return Err("the period is empty: it ends when it starts".to_owned());
            }
        } else if self.end <= self.start {
            return Err("the period ends before it starts".to_owned());
        }
        Ok(())
    }

    pub fn contains(&self, at: DateTime) -> bool {
        if !self.every_year {
            return self.start <= at && at < self.end;
        }
        let (start, end, at) = (in_year(self.start), in_year(self.end), in_year(at));
        if start < end {
            start <= at && at < end
        } else {
            // Over the new year.
            at >= start || at < end
        }
    }

    /// When the period last started, at or before `at` (which it contains).
    fn started(&self, at: DateTime) -> DateTime {
        if !self.every_year {
            return self.start;
        }
        let year = if in_year(self.start) <= in_year(at) {
            at.year()
        } else {
            at.year() - 1
        };
        // 29 February in a year without one: the 28th.
        let date = Date::new(year, self.start.month(), self.start.day())
            .or_else(|_| Date::new(year, self.start.month(), self.start.day() - 1))
            .unwrap_or(at.date());
        date.to_datetime(self.start.time())
    }
}

/// The schedule playing at `at` (station time): the other schedule whose
/// period started last among those containing `at`, else `default`.
pub fn playing<'a>(default: &'a Schedule, others: &'a [Schedule], at: DateTime) -> &'a Schedule {
    let mut best: Option<(DateTime, &Schedule)> = None;
    for schedule in others {
        for period in schedule.periods.iter().filter(|p| p.contains(at)) {
            let started = period.started(at);
            if best.is_none_or(|(when, _)| started > when) {
                best = Some((started, schedule));
            }
        }
    }
    best.map_or(default, |(_, schedule)| schedule)
}

/// When the schedule playing changes after `at`, and to which, looking up to
/// `horizon_hours` ahead (hour by hour: periods change on the hour).
pub fn next_change<'a>(
    default: &'a Schedule,
    others: &'a [Schedule],
    at: DateTime,
    horizon_hours: i64,
) -> Option<(DateTime, &'a Schedule)> {
    let now = playing(default, others, at).id;
    let mut hour = at
        .with()
        .minute(0)
        .second(0)
        .subsec_nanosecond(0)
        .build()
        .ok()?;
    for _ in 0..horizon_hours {
        hour = hour.checked_add(jiff::Span::new().hours(1)).ok()?;
        let then = playing(default, others, hour);
        if then.id != now {
            return Some((hour, then));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use jiff::civil::datetime;

    use super::*;

    fn schedule(id: ScheduleId, periods: Vec<Period>) -> Schedule {
        Schedule {
            id,
            name: format!("s{id}"),
            hours: HashMap::new(),
            periods,
        }
    }

    fn christmas() -> Period {
        Period {
            start: datetime(2026, 12, 1, 0, 0, 0, 0),
            end: datetime(2027, 1, 1, 0, 0, 0, 0),
            every_year: true,
        }
    }

    #[test]
    fn a_yearly_period_comes_back_every_year() {
        let default = schedule(1, vec![]);
        let others = [schedule(2, vec![christmas()])];
        for (at, id) in [
            (datetime(2026, 11, 30, 23, 59, 0, 0), 1),
            (datetime(2026, 12, 1, 0, 0, 0, 0), 2),
            (datetime(2030, 12, 31, 23, 59, 0, 0), 2),
            (datetime(2031, 1, 1, 0, 0, 0, 0), 1),
            (datetime(2024, 12, 15, 12, 0, 0, 0), 2),
        ] {
            assert_eq!(playing(&default, &others, at).id, id, "{at}");
        }
    }

    #[test]
    fn a_yearly_period_runs_over_the_new_year() {
        let winter = Period {
            start: datetime(2026, 12, 20, 0, 0, 0, 0),
            end: datetime(2026, 1, 6, 0, 0, 0, 0),
            every_year: true,
        };
        assert!(winter.check().is_ok());
        assert!(winter.contains(datetime(2028, 12, 25, 0, 0, 0, 0)));
        assert!(winter.contains(datetime(2028, 1, 5, 23, 0, 0, 0)));
        assert!(!winter.contains(datetime(2028, 1, 6, 0, 0, 0, 0)));
        assert!(!winter.contains(datetime(2028, 7, 1, 0, 0, 0, 0)));
    }

    #[test]
    fn a_dated_period_plays_once() {
        let once = Period {
            every_year: false,
            ..christmas()
        };
        assert!(once.contains(datetime(2026, 12, 24, 0, 0, 0, 0)));
        assert!(!once.contains(datetime(2027, 12, 24, 0, 0, 0, 0)));
    }

    #[test]
    fn the_period_that_started_last_plays() {
        let default = schedule(1, vec![]);
        let eve = Period {
            start: datetime(2026, 12, 31, 20, 0, 0, 0),
            end: datetime(2027, 1, 1, 2, 0, 0, 0),
            every_year: false,
        };
        // Christmas until 1 January 06:00 this time, so it comes back.
        let long = Period {
            end: datetime(2027, 1, 1, 6, 0, 0, 0),
            ..christmas()
        };
        let others = [schedule(3, vec![eve]), schedule(2, vec![long])];
        for (at, id) in [
            (datetime(2026, 12, 31, 19, 0, 0, 0), 2),
            (datetime(2026, 12, 31, 20, 0, 0, 0), 3),
            (datetime(2027, 1, 1, 1, 59, 0, 0), 3),
            (datetime(2027, 1, 1, 2, 0, 0, 0), 2),
            (datetime(2027, 1, 1, 6, 0, 0, 0), 1),
        ] {
            assert_eq!(playing(&default, &others, at).id, id, "{at}");
        }
    }

    #[test]
    fn periods_start_and_end_on_the_hour() {
        let half = Period {
            start: datetime(2026, 12, 1, 0, 30, 0, 0),
            ..christmas()
        };
        assert!(half.check().is_err());
        let backwards = Period {
            start: datetime(2027, 1, 2, 0, 0, 0, 0),
            every_year: false,
            ..christmas()
        };
        assert!(backwards.check().is_err());
        let empty = Period {
            end: datetime(2030, 12, 1, 0, 0, 0, 0),
            ..christmas()
        };
        assert!(empty.check().is_err());
        assert!(christmas().check().is_ok());
    }

    #[test]
    fn the_next_change_is_on_the_hour() {
        let default = schedule(1, vec![]);
        let others = [schedule(2, vec![christmas()])];
        let (at, to) =
            next_change(&default, &others, datetime(2026, 11, 30, 22, 15, 0, 0), 48).unwrap();
        assert_eq!((at, to.id), (datetime(2026, 12, 1, 0, 0, 0, 0), 2));
        assert!(next_change(&default, &others, datetime(2026, 6, 1, 0, 0, 0, 0), 24).is_none());
    }

    #[test]
    fn a_leap_day_start_still_orders() {
        let leap = Period {
            start: datetime(2028, 2, 29, 0, 0, 0, 0),
            end: datetime(2028, 3, 2, 0, 0, 0, 0),
            every_year: true,
        };
        assert_eq!(
            leap.started(datetime(2027, 3, 1, 0, 0, 0, 0)),
            datetime(2027, 2, 28, 0, 0, 0, 0)
        );
    }
}
