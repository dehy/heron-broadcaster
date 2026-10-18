// SPDX-License-Identifier: AGPL-3.0-or-later

//! Restrictions: hard rules on tracks, artists and categories, never
//! relaxed (`docs/rotation.md#restrictions`).
//!
//! - A ban rule forbids the track while it matches.
//! - Allow rules restrict the track to where they match: for each target
//!   and kind, at least one of them must match.
//! - Times are the track's start, in the station's time zone.

use jiff::civil::{Date, DateTime};
use serde::{Deserialize, Serialize};

use crate::{ArtistId, CategoryId, TrackId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target {
    Track(TrackId),
    Artist(ArtistId),
    Category(CategoryId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Only where the rule matches.
    Allow,
    /// Never where the rule matches.
    Ban,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    /// Days of the week and hours of the day.
    Daypart {
        mode: Mode,
        /// ISO 8601: 1 = Monday … 7 = Sunday.
        days: Vec<u8>,
        /// 0 to 23.
        start_hour: u8,
        /// 1 to 24, excluded. At or before `start_hour`, the hours run past
        /// midnight and belong to the day they start on: Friday 22 to 2
        /// covers Saturday 01:30.
        end_hour: u8,
    },
    /// Dates, both included.
    DateRange {
        mode: Mode,
        #[cfg_attr(feature = "openapi", schema(value_type = String, format = Date))]
        start: Date,
        #[cfg_attr(feature = "openapi", schema(value_type = String, format = Date))]
        end: Date,
        /// Every year on these days and months (the years are ignored);
        /// the range may run over the new year.
        every_year: bool,
    },
    /// Never directly after a track of this category.
    NotAfter { category_id: CategoryId },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Restriction {
    pub id: u64,
    pub target: Target,
    pub rule: Rule,
}

/// What a restriction is checked against.
pub struct Subject<'a> {
    pub track: TrackId,
    pub artists: &'a [ArtistId],
    pub categories: &'a [CategoryId],
}

impl Subject<'_> {
    pub fn is(&self, target: Target) -> bool {
        match target {
            Target::Track(id) => self.track == id,
            Target::Artist(id) => self.artists.contains(&id),
            Target::Category(id) => self.categories.contains(&id),
        }
    }
}

impl Rule {
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Daypart {
                days,
                start_hour,
                end_hour,
                ..
            } => {
                if days.is_empty() || days.iter().any(|day| !(1..=7).contains(day)) {
                    return Err("days are 1 (Monday) to 7 (Sunday), at least one".to_owned());
                }
                if *start_hour > 23 || !(1..=24).contains(end_hour) {
                    return Err("hours start at 0 to 23 and end at 1 to 24".to_owned());
                }
                if start_hour == end_hour {
                    return Err("the hours are empty: use the days alone with 0 to 24".to_owned());
                }
                Ok(())
            }
            Self::DateRange {
                start,
                end,
                every_year,
                ..
            } => {
                if !every_year && start > end {
                    return Err("the range ends before it starts".to_owned());
                }
                Ok(())
            }
            Self::NotAfter { .. } => Ok(()),
        }
    }

    fn mode(&self) -> Mode {
        match self {
            Self::Daypart { mode, .. } | Self::DateRange { mode, .. } => *mode,
            Self::NotAfter { .. } => Mode::Ban,
        }
    }

    /// Whether the rule matches a track starting at `at` (station time)
    /// after a track of `previous` categories.
    fn matches(&self, at: DateTime, previous: Option<&[CategoryId]>) -> bool {
        match self {
            Self::Daypart {
                days,
                start_hour,
                end_hour,
                ..
            } => {
                let day = at.date().weekday().to_monday_one_offset() as u8;
                let hour = at.hour() as u8;
                if start_hour < end_hour {
                    days.contains(&day) && (*start_hour..*end_hour).contains(&hour)
                } else if hour >= *start_hour {
                    days.contains(&day)
                } else if hour < *end_hour {
                    let yesterday = if day == 1 { 7 } else { day - 1 };
                    days.contains(&yesterday)
                } else {
                    false
                }
            }
            Self::DateRange {
                start,
                end,
                every_year,
                ..
            } => {
                let date = at.date();
                if *every_year {
                    let day = (date.month(), date.day());
                    let (from, to) = ((start.month(), start.day()), (end.month(), end.day()));
                    if from <= to {
                        from <= day && day <= to
                    } else {
                        day >= from || day <= to
                    }
                } else {
                    *start <= date && date <= *end
                }
            }
            Self::NotAfter { category_id } => {
                previous.is_some_and(|categories| categories.contains(category_id))
            }
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Self::Daypart { .. } => 0,
            Self::DateRange { .. } => 1,
            Self::NotAfter { .. } => 2,
        }
    }
}

/// The first restriction that forbids `subject` at `at`, after a track of
/// `previous` categories (`None` when unknown).
pub fn forbidding<'r>(
    restrictions: &'r [Restriction],
    subject: &Subject<'_>,
    at: DateTime,
    previous: Option<&[CategoryId]>,
) -> Option<&'r Restriction> {
    // Allow rules: one per (target, kind) must match.
    let mut groups: Vec<(Target, u8, bool, &Restriction)> = Vec::new();
    for restriction in restrictions.iter().filter(|r| subject.is(r.target)) {
        let matches = restriction.rule.matches(at, previous);
        match restriction.rule.mode() {
            Mode::Ban if matches => return Some(restriction),
            Mode::Ban => {}
            Mode::Allow => {
                let key = (restriction.target, restriction.rule.kind());
                match groups.iter_mut().find(|(t, k, ..)| (*t, *k) == key) {
                    Some(group) => group.2 |= matches,
                    None => groups.push((key.0, key.1, matches, restriction)),
                }
            }
        }
    }
    groups
        .into_iter()
        .find(|(.., matched, _)| !matched)
        .map(|(.., restriction)| restriction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn subject() -> Subject<'static> {
        Subject {
            track: 1,
            artists: &[10],
            categories: &[100],
        }
    }

    fn daypart(mode: Mode, days: &[u8], start_hour: u8, end_hour: u8) -> Rule {
        Rule::Daypart {
            mode,
            days: days.to_vec(),
            start_hour,
            end_hour,
        }
    }

    fn restriction(id: u64, target: Target, rule: Rule) -> Restriction {
        Restriction { id, target, rule }
    }

    // 2026-10-16 is a Friday.
    fn friday(hour: i8) -> DateTime {
        date(2026, 10, 16).at(hour, 30, 0, 0)
    }

    #[test]
    fn bans_apply_while_they_match() {
        // No explicit lyrics before 22:00.
        let rules = [restriction(
            1,
            Target::Artist(10),
            daypart(Mode::Ban, &[1, 2, 3, 4, 5, 6, 7], 6, 22),
        )];
        assert!(forbidding(&rules, &subject(), friday(21), None).is_some());
        assert!(forbidding(&rules, &subject(), friday(22), None).is_none());
        assert!(forbidding(&rules, &subject(), friday(5), None).is_none());
        // Other artists are not concerned.
        let other = Subject {
            artists: &[11],
            ..subject()
        };
        assert!(forbidding(&rules, &other, friday(21), None).is_none());
    }

    #[test]
    fn night_hours_belong_to_the_day_they_start() {
        let rules = [restriction(
            1,
            Target::Track(1),
            daypart(Mode::Allow, &[5], 22, 2),
        )];
        assert!(forbidding(&rules, &subject(), friday(23), None).is_none());
        // Saturday 01:30, in Friday's night.
        let saturday = date(2026, 10, 17).at(1, 30, 0, 0);
        assert!(forbidding(&rules, &subject(), saturday, None).is_none());
        // Friday 01:30 belongs to Thursday's night.
        assert!(forbidding(&rules, &subject(), friday(1), None).is_some());
        assert!(forbidding(&rules, &subject(), friday(12), None).is_some());
    }

    #[test]
    fn allow_rules_of_a_target_add_up() {
        let rules = [
            restriction(1, Target::Category(100), daypart(Mode::Allow, &[5], 6, 9)),
            restriction(2, Target::Category(100), daypart(Mode::Allow, &[5], 17, 19)),
        ];
        assert!(forbidding(&rules, &subject(), friday(7), None).is_none());
        assert!(forbidding(&rules, &subject(), friday(18), None).is_none());
        assert_eq!(
            forbidding(&rules, &subject(), friday(12), None).map(|r| r.id),
            Some(1)
        );
    }

    #[test]
    fn allow_rules_of_different_targets_all_apply() {
        let rules = [
            restriction(1, Target::Category(100), daypart(Mode::Allow, &[5], 6, 20)),
            restriction(2, Target::Artist(10), daypart(Mode::Allow, &[5], 12, 24)),
        ];
        assert!(forbidding(&rules, &subject(), friday(15), None).is_none());
        assert!(forbidding(&rules, &subject(), friday(8), None).is_some());
        assert!(forbidding(&rules, &subject(), friday(21), None).is_some());
    }

    #[test]
    fn seasons_come_back_every_year() {
        let christmas = Rule::DateRange {
            mode: Mode::Allow,
            start: date(2000, 12, 1),
            end: date(2000, 1, 6),
            every_year: true,
        };
        let rules = [restriction(1, Target::Category(100), christmas)];
        let on = |y, m, d| forbidding(&rules, &subject(), date(y, m, d).at(12, 0, 0, 0), None);
        assert!(on(2026, 12, 24).is_none());
        assert!(on(2027, 1, 3).is_none());
        assert!(on(2026, 7, 14).is_some());
        let banned = Rule::DateRange {
            mode: Mode::Ban,
            start: date(2026, 10, 1),
            end: date(2026, 10, 31),
            every_year: false,
        };
        let rules = [restriction(2, Target::Track(1), banned)];
        assert!(forbidding(&rules, &subject(), friday(12), None).is_some());
        assert!(forbidding(&rules, &subject(), date(2026, 11, 1).at(0, 0, 0, 0), None).is_none());
    }

    #[test]
    fn sequences_look_at_the_previous_track() {
        // No ballad after a ballad.
        let rules = [restriction(
            1,
            Target::Category(100),
            Rule::NotAfter { category_id: 100 },
        )];
        assert!(forbidding(&rules, &subject(), friday(12), Some(&[100, 7])).is_some());
        assert!(forbidding(&rules, &subject(), friday(12), Some(&[7])).is_none());
        assert!(forbidding(&rules, &subject(), friday(12), None).is_none());
    }

    #[test]
    fn rules_are_checked() {
        assert!(daypart(Mode::Ban, &[], 1, 2).check().is_err());
        assert!(daypart(Mode::Ban, &[8], 1, 2).check().is_err());
        assert!(daypart(Mode::Ban, &[1], 24, 2).check().is_err());
        assert!(daypart(Mode::Ban, &[1], 3, 3).check().is_err());
        assert!(daypart(Mode::Ban, &[1], 0, 24).check().is_ok());
        let json = r#"{"kind": "date_range", "mode": "ban", "start": "2026-12-01",
                       "end": "2026-11-01", "every_year": false}"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert!(rule.check().is_err());
        let rule: Rule =
            serde_json::from_str(r#"{"kind": "not_after", "category_id": 3}"#).unwrap();
        assert_eq!(rule, Rule::NotAfter { category_id: 3 });
    }
}
