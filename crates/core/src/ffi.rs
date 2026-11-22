//! What the bindings to other languages need beyond the attributes on the
//! types themselves.

use std::time::{Duration, SystemTime};

use crate::{
    BundledNetwork, Departure, Error, GlobalId, LegEnd, LocalTime, Location, Network, Timestamp,
    Trip,
};

/// The name of [`Timestamp`] in the bindings. The Python generator of `UniFFI` uses
/// the name `Timestamp` itself, and type checkers lose the type of every
/// time if both exist.
type UtcTime = Timestamp;

// A timestamp crosses the boundary as the time type of each language, e.g.
// as a `datetime` in Python.
uniffi::custom_type!(UtcTime, SystemTime, {
    lower: |time| {
        let seconds = Duration::from_secs(time.unix_seconds.unsigned_abs());
        let epoch = SystemTime::UNIX_EPOCH;
        if time.unix_seconds >= 0 {
            epoch.checked_add(seconds)
        } else {
            epoch.checked_sub(seconds)
        }
        // Not reachable for times the library parsed or accepted, which are
        // within the years 1 to 9999.
        .unwrap_or(epoch)
    },
    try_lift: |time| {
        let seconds = match time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_secs())?,
            // Round down, as for times after 1970.
            Err(before) => {
                let before = before.duration();
                -i64::try_from(before.as_secs())? - i64::from(before.subsec_nanos() > 0)
            }
        };
        Ok(Timestamp { unix_seconds: seconds })
    },
});

// A local time crosses the boundary as text in ISO 8601 format, such as
// `2026-11-19T14:08:00+01:00`, which uniffi.toml turns into the type a
// language has for a time with an offset.
uniffi::custom_type!(LocalTime, String, {
    lower: |time| time.iso_8601(),
    try_lift: |_text| {
        uniffi::deps::anyhow::bail!("nothing takes a local time")
    },
});

// These methods return references or `usize` in Rust, which cannot cross the
// boundary. The bindings get them under the same names with owned values.

#[uniffi::export]
impl BundledNetwork {
    /// The ID of the network, such as `vvo-efa`.
    #[uniffi::method(name = "id")]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "UniFFI hands a method its receiver by reference"
    )]
    fn ffi_id(&self) -> String {
        self.id().to_owned()
    }

    /// The network, as the bundled registry has it.
    #[uniffi::method(name = "network")]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "UniFFI hands a method its receiver by reference"
    )]
    fn ffi_network(&self) -> Network {
        self.network()
    }
}

#[uniffi::export]
impl Location {
    /// The stations this location belongs to, one for each of its global
    /// IDs. A station and its platforms share them, so they tell whether
    /// two networks mean the same station.
    #[uniffi::method(name = "station_ids")]
    fn ffi_station_ids(&self) -> Vec<GlobalId> {
        self.station_ids()
    }

    /// Whether the two locations are the same station or parts of it, by
    /// their station IDs. `false` if either has no global ID.
    #[uniffi::method(name = "same_station")]
    fn ffi_same_station(&self, other: &Self) -> bool {
        self.same_station(other)
    }

    /// The time on the clocks at this location, with the offset from UTC
    /// that holds there at that time. `None` if the location has no time
    /// zone or one that is not a known IANA name, so that no time is shown
    /// in a zone the stop is not in. A network written by hand gives its
    /// locations a time zone through its `timezone`.
    #[uniffi::method(name = "local_time")]
    fn ffi_local_time(&self, time: UtcTime) -> Option<LocalTime> {
        self.local_time(time)
    }
}

#[uniffi::export]
impl Trip {
    /// Where and when the trip starts.
    #[uniffi::method(name = "departure")]
    fn ffi_departure(&self) -> Option<LegEnd> {
        self.departure().cloned()
    }

    /// Where and when the trip ends.
    #[uniffi::method(name = "arrival")]
    fn ffi_arrival(&self) -> Option<LegEnd> {
        self.arrival().cloned()
    }

    /// How often the traveler changes vehicles: the number of rides minus
    /// one, and zero for a trip without a ride. Walks are not rides.
    #[uniffi::method(name = "transfers")]
    fn ffi_transfers(&self) -> u32 {
        u32::try_from(self.transfers()).unwrap_or(u32::MAX)
    }

    /// How long the trip takes, in seconds: from the expected time of its
    /// departure to the expected time of its arrival. A walk has no
    /// real-time data, so a trip that starts or ends with one counts that
    /// end as planned, even if the ride next to it is late.
    #[uniffi::method(name = "duration_seconds")]
    fn ffi_duration_seconds(&self) -> Option<i64> {
        self.duration_seconds()
    }
}

#[uniffi::export]
impl Departure {
    /// The platform to show: the one the server knows now if there is one,
    /// otherwise the one of the timetable.
    #[uniffi::method(name = "platform")]
    fn ffi_platform(&self) -> Option<String> {
        self.platform().map(str::to_owned)
    }
}

#[uniffi::export]
impl LegEnd {
    /// The platform to show: the one the server knows now if there is one,
    /// otherwise the one of the timetable.
    #[uniffi::method(name = "platform")]
    fn ffi_platform(&self) -> Option<String> {
        self.platform().map(str::to_owned)
    }
}

/// Reads a network from an entry in the JSON format of the transport-apis
/// repository, see `Network::from_json` in the Rust documentation. A record
/// cannot have a constructor in the bindings, so this is a function.
#[uniffi::export]
fn network_from_json(id: &str, json: &str) -> Result<Network, Error> {
    Network::from_json(id, json)
}
