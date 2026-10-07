//! The current time, read once in `main` and passed down. Domain code never reads the clock.

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, SecondsFormat};
use serde::{Deserialize, Serialize};

/// The moment a command runs, in the user's local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now(DateTime<FixedOffset>);

impl Now {
    pub fn from_system_clock() -> Self {
        Self(Local::now().fixed_offset())
    }

    /// Parses a time like `2026-10-07T09:00:00+02:00`.
    pub fn parse(text: &str) -> Result<Self> {
        DateTime::parse_from_rfc3339(text)
            .map(Self)
            .with_context(|| format!("'{text}' is not a time like 2026-10-07T09:00:00+02:00"))
    }

    /// The user's calendar day.
    pub fn today(&self) -> Date {
        Date(self.0.date_naive())
    }

    /// How times are stored: local time with its offset, to the second.
    pub fn timestamp(&self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Secs, false)
    }
}

/// A calendar day, written like `2026-10-08`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Date(NaiveDate);

impl Date {
    /// The date with its day of the week, like `Thu 2026-10-08`, so that a wrong date
    /// is seen at once.
    pub fn with_weekday(&self) -> String {
        self.0.format("%a %Y-%m-%d").to_string()
    }
}

impl std::str::FromStr for Date {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .filter(|_| text.len() == 10)
            .map(Self)
            .with_context(|| format!("'{text}' is not a date like 2026-10-08"))
    }
}

impl TryFrom<String> for Date {
    type Error = anyhow::Error;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

impl From<Date> for String {
    fn from(date: Date) -> String {
        date.to_string()
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_is_written_as_year_month_day() {
        let date: Date = "2026-10-08".parse().unwrap();
        assert_eq!(date.to_string(), "2026-10-08");
        assert_eq!(serde_json::to_string(&date).unwrap(), "\"2026-10-08\"");
        assert!(date < "2026-10-09".parse().unwrap());
        assert_eq!(date.with_weekday(), "Thu 2026-10-08");
    }

    #[test]
    fn expires_on_must_be_a_date() {
        for text in [
            "tomorrow",
            "2026-13-01",
            "08.10.2026",
            "2026-10-08T09:00:00+02:00",
            "",
        ] {
            let error = text.parse::<Date>().unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("'{text}' is not a date like 2026-10-08")
            );
        }
    }

    #[test]
    fn today_is_the_local_date_of_now() {
        let now = Now::parse("2026-10-07T00:30:00+02:00").unwrap();
        assert_eq!(now.today(), "2026-10-07".parse().unwrap());
    }

    #[test]
    fn timestamp_keeps_the_local_date_and_offset() {
        let now = Now::parse("2026-10-07T00:30:00.123+02:00").unwrap();
        assert_eq!(now.timestamp(), "2026-10-07T00:30:00+02:00");
    }

    #[test]
    fn text_that_is_not_a_time_is_rejected() {
        assert!(Now::parse("tomorrow").is_err());
    }
}
