//! Safety net: a weekday written next to a date must be the day of that date. Models get
//! "Friday" wrong, so `nv add` refuses "Friday 2026-10-10" (a Saturday).

use std::sync::LazyLock;

use regex::Regex;

use crate::clock::Date;

/// A weekday and a date that do not belong together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrongWeekday {
    /// The words as written, like `Friday 2026-10-10`.
    pub said: String,
    pub date: Date,
    /// The day the date really is: `Saturday`.
    pub actual: &'static str,
}

// Full names in any case; short forms (Mon, Fri) only with a capital, so that "sat" and
// "sun" in a sentence are left alone. The text is English.
const WEEKDAY: &str = r"(?i:monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b|\b(?:Mon|Tue|Tues|Wed|Thu|Thur|Thurs|Fri|Sat|Sun)\b";
const DATE: &str = r"\d{4}-\d{2}-\d{2}\b";

/// `Friday 2026-10-10`, `Fri, 2026-10-10`, `Friday (2026-10-10)`.
static WEEKDAY_FIRST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\b({WEEKDAY})\.?,?\s*\(?\s*({DATE})\)?")).unwrap());
/// `2026-10-10 (Friday)`.
static DATE_FIRST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\b({DATE})\s*\(\s*({WEEKDAY})\s*\)")).unwrap());

/// The first weekday in `text` that stands next to a date of another day, if any.
pub fn find_wrong_weekday(text: &str) -> Option<WrongWeekday> {
    let weekday_first = WEEKDAY_FIRST.captures_iter(text).map(|found| {
        (
            found.get(0).unwrap(),
            found[1].to_string(),
            found[2].to_string(),
        )
    });
    let date_first = DATE_FIRST.captures_iter(text).map(|found| {
        (
            found.get(0).unwrap(),
            found[2].to_string(),
            found[1].to_string(),
        )
    });
    let mut pairs: Vec<_> = weekday_first.chain(date_first).collect();
    pairs.sort_by_key(|(whole, _, _)| whole.start());
    pairs.into_iter().find_map(|(whole, weekday, date)| {
        // A date like 2026-02-30 does not exist: not this check's business.
        let date: Date = date.parse().ok()?;
        let said_prefix = weekday.get(..3)?.to_lowercase();
        let actual = date.weekday_name();
        (!actual.to_lowercase().starts_with(&said_prefix)).then(|| WrongWeekday {
            said: whole.as_str().to_string(),
            date,
            actual,
        })
    })
}

#[cfg(test)]
#[path = "weekday_check_tests.rs"]
mod tests;
