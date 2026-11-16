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
        jiff::Timestamp::from_second(self.unix_seconds).map_err(|_| Error::InvalidQuery {
            message: "the time is outside the supported range".to_owned(),
        })
    }
}
