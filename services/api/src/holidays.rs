//! Reference-only US holiday markers for the calendar.
//!
//! Computed the same way the moon phase and sun times are (see [`crate::weather`]):
//! a closed-form calculation, no database row, no admin data entry, correct
//! for any year including ones far in the future. Unlike [`crate::events`],
//! nothing here is created by an admin and nothing here touches booking
//! availability or capacity — a holiday is a label on a day, not a fact about
//! the camp.

use axum::{Json, extract::Query};
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Holiday {
    pub date: NaiveDate,
    pub name: &'static str,
}

/// The `n`th `weekday` of `month` in `year` — e.g. the 3rd Monday of January.
/// `n` is 1-based.
fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid calendar month");
    let offset = (7 + weekday.num_days_from_sunday() as i64
        - first.weekday().num_days_from_sunday() as i64)
        % 7;
    first + Duration::days(offset + 7 * i64::from(n - 1))
}

/// The last `weekday` of `month` in `year` — e.g. the last Monday of May.
fn last_weekday_of_month(year: i32, month: u32, weekday: Weekday) -> NaiveDate {
    let next_month_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid calendar month");
    let last_day = next_month_first - Duration::days(1);
    let back = (last_day.weekday().num_days_from_sunday() as i64
        - weekday.num_days_from_sunday() as i64
        + 7)
        % 7;
    last_day - Duration::days(back)
}

/// The standard US holiday set for `year`, in calendar order.
pub fn us_holidays(year: i32) -> Vec<Holiday> {
    vec![
        Holiday {
            date: NaiveDate::from_ymd_opt(year, 1, 1).expect("Jan 1 is always valid"),
            name: "New Year's Day",
        },
        Holiday {
            date: nth_weekday(year, 1, Weekday::Mon, 3),
            name: "MLK Day",
        },
        Holiday {
            date: nth_weekday(year, 2, Weekday::Mon, 3),
            name: "Presidents Day",
        },
        Holiday {
            date: last_weekday_of_month(year, 5, Weekday::Mon),
            name: "Memorial Day",
        },
        Holiday {
            date: NaiveDate::from_ymd_opt(year, 6, 19).expect("Jun 19 is always valid"),
            name: "Juneteenth",
        },
        Holiday {
            date: NaiveDate::from_ymd_opt(year, 7, 4).expect("Jul 4 is always valid"),
            name: "Independence Day",
        },
        Holiday {
            date: nth_weekday(year, 9, Weekday::Mon, 1),
            name: "Labor Day",
        },
        Holiday {
            date: NaiveDate::from_ymd_opt(year, 11, 11).expect("Nov 11 is always valid"),
            name: "Veterans Day",
        },
        Holiday {
            date: nth_weekday(year, 11, Weekday::Thu, 4),
            name: "Thanksgiving",
        },
        Holiday {
            date: NaiveDate::from_ymd_opt(year, 12, 25).expect("Dec 25 is always valid"),
            name: "Christmas Day",
        },
    ]
}

#[derive(Debug, Deserialize)]
pub struct HolidayQuery {
    pub year: Option<i32>,
}

/// `GET /api/holidays?year=2026` — public, no auth. Defaults to the current
/// year. Pure computation, so there is nothing worth caching.
pub async fn list(Query(q): Query<HolidayQuery>) -> Json<Vec<Holiday>> {
    let year = q.year.unwrap_or_else(|| Utc::now().year());
    Json(us_holidays(year))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(holidays: &'a [Holiday], name: &str) -> &'a Holiday {
        holidays
            .iter()
            .find(|h| h.name == name)
            .unwrap_or_else(|| panic!("{name} missing from the holiday set"))
    }

    #[test]
    fn exactly_ten_holidays() {
        assert_eq!(us_holidays(2026).len(), 10);
    }

    #[test]
    fn fixed_dates_never_move() {
        let h = us_holidays(2026);
        assert_eq!(
            find(&h, "New Year's Day").date,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
        );
        assert_eq!(
            find(&h, "Juneteenth").date,
            NaiveDate::from_ymd_opt(2026, 6, 19).unwrap()
        );
        assert_eq!(
            find(&h, "Independence Day").date,
            NaiveDate::from_ymd_opt(2026, 7, 4).unwrap()
        );
        assert_eq!(
            find(&h, "Veterans Day").date,
            NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
        );
        assert_eq!(
            find(&h, "Christmas Day").date,
            NaiveDate::from_ymd_opt(2026, 12, 25).unwrap()
        );
    }

    /// Spot checks for the current year (2026), as the addendum asks for.
    #[test]
    fn thanksgiving_2026_is_november_26() {
        let h = us_holidays(2026);
        let t = find(&h, "Thanksgiving");
        assert_eq!(t.date, NaiveDate::from_ymd_opt(2026, 11, 26).unwrap());
        assert_eq!(t.date.weekday(), Weekday::Thu);
    }

    #[test]
    fn memorial_day_2026_is_may_25() {
        let h = us_holidays(2026);
        let m = find(&h, "Memorial Day");
        assert_eq!(m.date, NaiveDate::from_ymd_opt(2026, 5, 25).unwrap());
        assert_eq!(m.date.weekday(), Weekday::Mon);
        // The whole point of "last Monday" — the following Monday must fall
        // in June.
        assert_eq!((m.date + Duration::days(7)).month(), 6);
    }

    #[test]
    fn floating_holidays_land_on_the_right_weekday() {
        let h = us_holidays(2026);
        assert_eq!(find(&h, "MLK Day").date.weekday(), Weekday::Mon);
        assert_eq!(find(&h, "Presidents Day").date.weekday(), Weekday::Mon);
        assert_eq!(find(&h, "Labor Day").date.weekday(), Weekday::Mon);
        assert_eq!(find(&h, "Thanksgiving").date.weekday(), Weekday::Thu);
    }

    /// Known-correct dates a year on either side of 2026, so the maths isn't
    /// silently tuned to one calendar's coincidences.
    #[test]
    fn known_correct_dates_across_years() {
        assert_eq!(
            find(&us_holidays(2027), "Thanksgiving").date,
            NaiveDate::from_ymd_opt(2027, 11, 25).unwrap()
        );
        assert_eq!(
            find(&us_holidays(2024), "Thanksgiving").date,
            NaiveDate::from_ymd_opt(2024, 11, 28).unwrap()
        );
        assert_eq!(
            find(&us_holidays(2000), "Labor Day").date,
            NaiveDate::from_ymd_opt(2000, 9, 4).unwrap()
        );
        assert_eq!(
            find(&us_holidays(2024), "Memorial Day").date,
            NaiveDate::from_ymd_opt(2024, 5, 27).unwrap()
        );
    }

    /// Works for a year far in the future with no data entry — the whole
    /// point of computing this instead of storing it.
    #[test]
    fn works_decades_out() {
        let h = us_holidays(2075);
        assert_eq!(h.len(), 10);
        assert_eq!(find(&h, "Thanksgiving").date.weekday(), Weekday::Thu);
        assert_eq!(find(&h, "Memorial Day").date.weekday(), Weekday::Mon);
    }

    #[test]
    fn holidays_come_back_in_calendar_order() {
        let h = us_holidays(2026);
        for pair in h.windows(2) {
            assert!(
                pair[0].date < pair[1].date,
                "{:?} not before {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn default_year_is_the_current_year() {
        let current = Utc::now().year();
        let h = us_holidays(current);
        assert_eq!(h.len(), 10);
    }
}
