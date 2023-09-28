//! Custom serialization methods used throughout the crate

pub mod duration_ms {
    use libs::chrono::Duration;
    use serde::{de, Serializer};
    use std::convert::TryFrom;
    use std::fmt;

    /// Vistor to help deserialize duration represented as millisecond to
    /// `chrono::Duration`.
    pub struct DurationVisitor;
    impl<'de> de::Visitor<'de> for DurationVisitor {
        type Value = Duration;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a milliseconds represents chrono::Duration")
        }
        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::milliseconds(v))
        }

        // JSON deserializer calls visit_u64 for non-negative intgers
        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            i64::try_from(v)
                .map(Duration::milliseconds)
                .map_err(E::custom)
        }
    }

    /// Deserialize `chrono::Duration` from milliseconds (represented as i64)
    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        d.deserialize_i64(DurationVisitor)
    }

    /// Serialize `chrono::Duration` to milliseconds (represented as i64)
    pub fn serialize<S>(x: &Duration, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_i64(x.num_milliseconds())
    }
}

pub mod millisecond_timestamp {
    use libs::chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{de, Serializer};
    use std::fmt;

    /// Vistor to help deserialize unix millisecond timestamp to
    /// `chrono::DateTime`.
    struct DateTimeVisitor;

    impl<'de> de::Visitor<'de> for DateTimeVisitor {
        type Value = DateTime<Utc>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(
                formatter,
                "an unix millisecond timestamp represents DataTime<UTC>"
            )
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let second = (v - v % 1000) / 1000;
            let nanosecond = ((v % 1000) * 1_000_000) as u32;
            // The maximum value of i64 is large enough to hold milliseconds,
            // so it would be safe to convert it i64.
            match NaiveDateTime::from_timestamp_opt(second as i64, nanosecond) {
                Some(ndt) => Ok(DateTime::<Utc>::from_utc(ndt, Utc)),
                None => Err(E::custom(format!("v is invalid second: {v}"))),
            }
        }
    }

    /// Deserialize Unix millisecond timestamp to `DateTime<Utc>`
    pub fn deserialize<'de, D>(d: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        d.deserialize_u64(DateTimeVisitor)
    }

    /// Serialize `DateTime<Utc>` to Unix millisecond timestamp
    pub fn serialize<S>(x: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_i64(x.timestamp_millis())
    }
}

pub mod option_duration_ms {
    use crate::custom_serde::duration_ms;
    use libs::chrono::Duration;
    use serde::{de, Serializer};
    use std::fmt;

    /// Vistor to help deserialize duration represented as milliseconds to
    /// `Option<chrono::Duration>`
    struct OptionDurationVisitor;

    impl<'de> de::Visitor<'de> for OptionDurationVisitor {
        type Value = Option<Duration>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(
                formatter,
                "a optional milliseconds represents chrono::Duration"
            )
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            Ok(Some(
                deserializer.deserialize_i64(duration_ms::DurationVisitor)?,
            ))
        }
    }

    /// Deserialize `Option<chrono::Duration>` from milliseconds
    /// (represented as i64)
    pub fn deserialize<'de, D>(d: D) -> Result<Option<Duration>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        d.deserialize_option(OptionDurationVisitor)
    }

    /// Serialize `Option<chrono::Duration>` to milliseconds (represented as
    /// i64)
    pub fn serialize<S>(x: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *x {
            Some(duration) => s.serialize_i64(duration.num_milliseconds()),
            None => s.serialize_none(),
        }
    }
}

pub mod duration_second {
    use libs::chrono::Duration;
    use serde::{de, Deserialize, Serializer};

    /// Deserialize `chrono::Duration` from seconds (represented as u64)
    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let duration: i64 = Deserialize::deserialize(d)?;
        Ok(Duration::seconds(duration))
    }

    /// Serialize `chrono::Duration` to seconds (represented as u64)
    pub fn serialize<S>(x: &Duration, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_i64(x.num_seconds())
    }
}

pub mod space_separated_scopes {
    use serde::{de, Deserialize, Serializer};
    use std::collections::HashSet;

    pub fn deserialize<'de, D>(d: D) -> Result<HashSet<String>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let scopes: &str = Deserialize::deserialize(d)?;
        Ok(scopes.split_whitespace().map(ToOwned::to_owned).collect())
    }

    pub fn serialize<S>(scopes: &HashSet<String>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let scopes = scopes.clone().into_iter().collect::<Vec<_>>().join(" ");
        s.serialize_str(&scopes)
    }
}
