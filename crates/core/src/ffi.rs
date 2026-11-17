//! What the bindings to other languages need beyond the attributes on the
//! types themselves.

use std::time::{Duration, SystemTime};

use crate::{Error, LegEnd, Location, Network, Timestamp, Trip};

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

// These methods return references or `usize` in Rust, which cannot cross the
// boundary. The bindings get them under the same names with owned values.

#[uniffi::export]
impl Location {
    /// The station this location belongs to: the first three parts of the
    /// global ID. A station and its platforms share it, so it tells whether
    /// two networks mean the same station.
    #[uniffi::method(name = "station_id")]
    fn ffi_station_id(&self) -> Option<String> {
        self.station_id().map(str::to_owned)
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

    /// The number of rides minus one. Walks do not count.
    #[uniffi::method(name = "transfers")]
    fn ffi_transfers(&self) -> u32 {
        u32::try_from(self.transfers()).unwrap_or(u32::MAX)
    }
}

/// Reads a network from an entry in the JSON format of the transport-apis
/// repository, see `Network::from_json` in the Rust documentation. A record
/// cannot have a constructor in the bindings, so this is a function.
#[uniffi::export]
fn network_from_json(id: &str, json: &str) -> Result<Network, Error> {
    Network::from_json(id, json)
}
