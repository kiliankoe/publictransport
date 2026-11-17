//! Conversions between [`Timestamp`] and the forms the protocols use.

use serde::{Deserialize, Deserializer};

use crate::{Error, Timestamp};

/// The times every protocol can write down and every language can hold, in
/// Unix seconds.
const YEARS_1_TO_9999: std::ops::Range<i64> = -62_135_596_800..253_402_300_800;

/// A time in RFC 3339 format, as both protocols send it.
pub(crate) struct WireTime(pub(crate) Timestamp);

impl<'de> Deserialize<'de> for WireTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let time: jiff::Timestamp = text.trim().parse().map_err(serde::de::Error::custom)?;
        let time = Timestamp {
            unix_seconds: time.as_second(),
        };
        // Other languages cannot hold times outside these years.
        if !YEARS_1_TO_9999.contains(&time.unix_seconds) {
            return Err(serde::de::Error::custom("time outside the years 1 to 9999"));
        }
        Ok(Self(time))
    }
}

impl Timestamp {
    pub(crate) fn to_jiff(self) -> Result<jiff::Timestamp, Error> {
        jiff::Timestamp::from_second(self.unix_seconds)
            .ok()
            .filter(|_| YEARS_1_TO_9999.contains(&self.unix_seconds))
            .ok_or_else(|| Error::InvalidQuery {
                message: "the time is outside the years 1 to 9999".to_owned(),
            })
    }
}
