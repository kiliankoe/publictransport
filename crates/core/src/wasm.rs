//! What the WebAssembly module in `bindings/wasm` needs beyond the
//! attributes on the types themselves.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use serde_wasm_bindgen::preserve;

use crate::{Coordinate, LocalTime, Location, Place, Timestamp};

/// Reads a field with a default, for which JavaScript may also say
/// `undefined`.
pub(crate) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::deserialize(deserializer)?.unwrap_or_default())
}

// A place is told apart by what it is in JavaScript: text is an ID, an
// object with an `id` a location, any other object a coordinate. An error
// then names what is wrong with that one kind.
impl<'de> Deserialize<'de> for Place {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Neither kind of object holds a time, which this could not carry.
        let place = match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(id) => Ok(Self::Id(id)),
            value @ serde_json::Value::Object(_) if value.get("id").is_some() => {
                Location::deserialize(value).map(Self::Location)
            }
            value @ serde_json::Value::Object(_) => {
                Coordinate::deserialize(value).map(Self::Coordinate)
            }
            _ => {
                return Err(D::Error::custom(
                    "a place is an ID, a coordinate or a location",
                ));
            }
        };
        place.map_err(D::Error::custom)
    }
}

// A timestamp crosses the boundary as a `Date`. The object itself is handed
// through, which only the serializer of serde-wasm-bindgen can do.
impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a `Date` holds fewer seconds than an `f64` holds exactly"
        )]
        let milliseconds = self.unix_seconds as f64 * 1000.0;
        preserve::serialize(&js_sys::Date::new(&milliseconds.into()), serializer)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let date: js_sys::Date = preserve::deserialize(deserializer)?;
        // Round down, also for times before 1970.
        let seconds = (date.get_time() / 1000.0).floor();
        if seconds.is_nan() {
            return Err(D::Error::custom("the date is invalid"));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a `Date` holds fewer seconds than an `i64`"
        )]
        let unix_seconds = seconds as i64;
        Ok(Self { unix_seconds })
    }
}

// A local time crosses the boundary as text in ISO 8601 format, because
// JavaScript has no type for a time with an offset.
impl Serialize for LocalTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.iso_8601())
    }
}
