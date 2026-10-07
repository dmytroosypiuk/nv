//! The current time, read once in `main` and passed down. Domain code never reads the clock.

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Local, SecondsFormat};

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

    /// How times are stored: local time with its offset, to the second.
    pub fn timestamp(&self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Secs, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
