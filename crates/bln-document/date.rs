//! Validated calendar dates for publication metadata.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A calendar date in exact YYYY-MM-DD form, without a time or timezone.
///
/// Canonical fixed-width representation makes lexical and calendar order identical.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PublicationDate(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected a valid calendar date (YYYY-MM-DD), got '{0}'")]
pub struct InvalidPublicationDate(String);

impl PublicationDate {
    /// Borrows the canonical calendar date.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PublicationDate {
    type Err = InvalidPublicationDate;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        let valid_shape = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
        if !valid_shape {
            return Err(InvalidPublicationDate(value.into()));
        }
        // Shape validation guarantees ASCII decimal fields.
        let year: u16 = value[..4].parse().unwrap();
        let month: u8 = value[5..7].parse().unwrap();
        let day: u8 = value[8..].parse().unwrap();
        let maximum = match month {
            2 if year.is_multiple_of(400)
                || (year.is_multiple_of(4) && !year.is_multiple_of(100)) =>
            {
                29
            }
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        if day == 0 || day > maximum {
            return Err(InvalidPublicationDate(value.into()));
        }
        Ok(Self(value.into()))
    }
}

impl TryFrom<String> for PublicationDate {
    type Error = InvalidPublicationDate;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<PublicationDate> for String {
    fn from(value: PublicationDate) -> Self {
        value.0
    }
}

impl fmt::Display for PublicationDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::PublicationDate;

    #[test]
    fn accepts_only_valid_calendar_dates() {
        for value in ["2024-02-29", "2000-02-29", "2026-12-31"] {
            assert_eq!(value.parse::<PublicationDate>().unwrap().as_str(), value);
        }
        for value in [
            "1900-02-29",
            "2025-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-13-01",
            "2026-01-00",
            "2026-1-01",
            "2026-01-01T12:00:00Z",
            "2026-01-01 garbage",
            "",
            "é026-01-01",
        ] {
            assert!(value.parse::<PublicationDate>().is_err(), "{value}");
        }
    }
}
