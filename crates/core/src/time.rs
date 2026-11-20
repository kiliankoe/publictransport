//! Conversions between [`Timestamp`] and the forms the protocols use.

use serde::{Deserialize, Deserializer};

use crate::{Error, LocalTime, Location, Network, Timestamp};

/// The times every protocol can write down and every language can hold, in
/// Unix seconds.
const YEARS_1_TO_9999: std::ops::Range<i64> = -62_135_596_800..253_402_300_800;

/// A time in RFC 3339 format, as MOTIS, EFA and `OpenTripPlanner` send it.
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

#[cfg(any(feature = "uniffi", feature = "unstable-wasm"))]
impl LocalTime {
    /// The time in ISO 8601 format, such as `2026-11-19T14:08:00+01:00`,
    /// which is how it crosses into other languages.
    pub(crate) fn iso_8601(self) -> String {
        let offset = self.utc_offset_seconds.unsigned_abs();
        // Zones had offsets with seconds before they agreed on whole minutes.
        let seconds = match offset % 60 {
            0 => String::new(),
            seconds => format!(":{seconds:02}"),
        };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}{seconds}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            if self.utc_offset_seconds < 0 {
                '-'
            } else {
                '+'
            },
            offset / 3600,
            offset / 60 % 60,
        )
    }
}

impl Location {
    /// `time` on the clocks at this location. `None` if the location has
    /// no [`Location::timezone`] or one that is not a known IANA name, so
    /// that no time is shown in a zone the stop is not in, and for a time
    /// outside the years 1 to 9999.
    ///
    /// Locations of the bundled networks have a time zone. A network
    /// written by hand gives its locations one through
    /// [`Network::timezone`], unless its server names one per stop.
    #[must_use]
    pub fn local_time(&self, time: Timestamp) -> Option<LocalTime> {
        let zone = jiff::tz::TimeZone::get(self.timezone.as_deref()?).ok()?;
        // The database has names for "not known", which act as UTC.
        if zone.is_unknown() {
            return None;
        }
        let local = time.to_jiff().ok()?.to_zoned_datetime(zone);
        // Other languages cannot hold the year before the year 1.
        let year = u16::try_from(local.year()).ok().filter(|year| *year >= 1)?;
        Some(LocalTime {
            year,
            month: u8::try_from(local.month()).ok()?,
            day: u8::try_from(local.day()).ok()?,
            hour: u8::try_from(local.hour()).ok()?,
            minute: u8::try_from(local.minute()).ok()?,
            second: u8::try_from(local.second()).ok()?,
            utc_offset_seconds: local.offset().seconds(),
        })
    }
}

impl Network {
    /// The time zone of the network, which the protocols with wall-clock
    /// times need.
    pub(crate) fn zone(&self) -> Result<jiff::tz::TimeZone, Error> {
        let zone = self
            .timezone
            .as_deref()
            .ok_or_else(|| Error::InvalidNetwork {
                message: "local times need the network's time zone".to_owned(),
            })?;
        jiff::tz::TimeZone::get(zone).map_err(|error| Error::InvalidNetwork {
            message: error.to_string(),
        })
    }

    /// The time on a clock in the network's time zone.
    pub(crate) fn local(&self, time: Timestamp) -> Result<jiff::ZonedDateTime, Error> {
        let local = time.to_jiff()?.to_zoned_datetime(self.zone()?);
        if !(1..=9999).contains(&local.year()) {
            return Err(Error::InvalidQuery {
                message: "the time is outside the years 1 to 9999".to_owned(),
            });
        }
        Ok(local)
    }
}
