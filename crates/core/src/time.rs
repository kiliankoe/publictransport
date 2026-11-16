//! Conversions between [`Timestamp`] and the forms the protocols use.

use serde::{Deserialize, Deserializer};

use crate::{Error, Timestamp};

/// A time in RFC 3339 format, as both protocols send it.
pub(crate) struct WireTime(pub(crate) Timestamp);

impl<'de> Deserialize<'de> for WireTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let time: jiff::Timestamp = text.parse().map_err(serde::de::Error::custom)?;
        Ok(Self(Timestamp {
            unix_seconds: time.as_second(),
        }))
    }
}

impl Timestamp {
    pub(crate) fn to_jiff(self) -> Result<jiff::Timestamp, Error> {
        // The years 1 to 9999, which every protocol can write down.
        let range = -62_135_596_800..253_402_300_800;
        jiff::Timestamp::from_second(self.unix_seconds)
            .ok()
            .filter(|_| range.contains(&self.unix_seconds))
            .ok_or_else(|| Error::InvalidQuery {
                message: "the time is outside the years 1 to 9999".to_owned(),
            })
    }
}
