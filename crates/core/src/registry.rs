//! Networks as data: entries in the format of the `transport-apis`
//! repository, and the set that ships with the crate.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{Coordinate, Error, HafasOptions, HafasProduct, Network, Protocol};

/// One of the kinds of request that a [`Network`] can be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum Operation {
    /// A search for locations.
    Locations,
    /// The departures at a stop, and with them the arrivals.
    Departures,
    /// Trips between two places.
    Trips,
    /// The stops near a position.
    Nearby,
    /// The run of one vehicle.
    Journeys,
}

/// Where a [`Network`] has data, in three tiers of quality.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Coverage {
    /// Where the network has real-time data.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime: Option<Area>,
    /// Where it has complete timetables.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub regular: Option<Area>,
    /// Where it has any data.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub any: Option<Area>,
}

/// A part of the world, described by region codes, by polygons or by both.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Area {
    /// ISO 3166 codes of countries (`DE`) or their subdivisions (`DE-SN`).
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, deserialize_with = "crate::wasm::or_default")
    )]
    pub regions: Vec<String>,
    /// The outlines of the area.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, deserialize_with = "crate::wasm::or_default")
    )]
    pub polygons: Vec<Polygon>,
}

/// A polygon as in `GeoJSON`: the first ring is the outline, further rings
/// are holes in it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Polygon {
    /// Each ring is a closed line of positions.
    pub rings: Vec<Vec<Coordinate>>,
}

/// A tier of a [`Coverage`], best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum CoverageTier {
    /// Real-time data.
    Realtime,
    /// Complete timetables.
    Regular,
    /// Any data.
    Any,
}

/// Whom to name as the source of a [`Network`]'s data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Attribution {
    /// The name of the source.
    pub name: String,
    /// Its website.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub homepage: Option<String>,
    /// The license of the data, where the source has a single one.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub license: Option<String>,
}

impl Attribution {
    /// An attribution with a name only.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            homepage: None,
            license: None,
        }
    }
}

impl Area {
    /// Whether one of the polygons contains the position.
    fn contains(&self, position: Coordinate) -> bool {
        self.polygons
            .iter()
            .any(|polygon| polygon.contains(position))
    }
}

impl Polygon {
    /// Counts the edges of all rings that a line from the position to the
    /// east crosses. An odd count means inside the outline and outside
    /// every hole.
    fn contains(&self, position: Coordinate) -> bool {
        let crossings = self
            .rings
            .iter()
            .flat_map(|ring| ring.iter().zip(ring.iter().cycle().skip(1)))
            .filter(|(from, to)| {
                (from.latitude > position.latitude) != (to.latitude > position.latitude) && {
                    let along = (position.latitude - from.latitude) / (to.latitude - from.latitude);
                    position.longitude < from.longitude + along * (to.longitude - from.longitude)
                }
            })
            .count();
        crossings % 2 == 1
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Coverage {
    /// The best tier that has a polygon containing the position. Region
    /// codes are not used: the crate has no borders to resolve them with.
    #[must_use]
    pub fn tier_at(&self, position: Coordinate) -> Option<CoverageTier> {
        [
            (&self.realtime, CoverageTier::Realtime),
            (&self.regular, CoverageTier::Regular),
            (&self.any, CoverageTier::Any),
        ]
        .into_iter()
        .find(|(area, _)| area.as_ref().is_some_and(|area| area.contains(position)))
        .map(|(_, tier)| tier)
    }
}

impl Network {
    /// Reads a network from an entry in the JSON format of the
    /// [transport-apis](https://github.com/public-transport/transport-apis)
    /// repository. `id` is the name of the entry's file without `.json`.
    ///
    /// The crate reads four fields that repository does not have: the type
    /// `ojp` with `options.ojpVersion`, `options.requestorRef`, and
    /// `usageTerms` for [`Network::terms`]. An entry of the type
    /// `otpGraphQl` is read if it is a Transmodel server, which the entry
    /// shows by `options.apiVersion` being `entur` or by `transmodel` in its
    /// endpoint. The crate reads [`Network::geocoder`] from the field
    /// `options.geocoder`, which that repository does not have, so its
    /// entries give a network without one. An entry of the type
    /// `hafasMgate` gives a network with [`Network::hafas`] that asks in the
    /// first of the entry's `supportedLanguages` unless the network has a
    /// [`Network::language`]. No entry gives a network one, because not
    /// every server honors the languages its entry lists. Its salts for signed requests are not read: the crate does not
    /// sign, and no server that was tried asks for it. For
    /// MOTIS the entry names the root of the server, and the endpoint gets
    /// `api` appended unless it ends in it.
    ///
    /// An entry is taken at its word. Whether the server answers is only
    /// known for the [bundled](Registry::bundled) networks. Entries from
    /// that repository often lack what a request needs later: EFA entries
    /// the time zone, TRIAS entries the requestor reference. The versions
    /// they declare are not checked either, because they are often out of
    /// date, and nothing there says whether an EFA server has the JSON
    /// output this crate uses.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetwork`] if the text is not such an entry,
    /// if its protocol is not one of this crate's, or if the server wants a
    /// key in an HTTP header.
    pub fn from_json(id: &str, json: &str) -> Result<Self, Error> {
        let invalid = |message: String| Error::InvalidNetwork { message };
        let entry: Entry =
            serde_json::from_str(json).map_err(|error| invalid(format!("{id}: {error}")))?;
        let mut options = entry.options;

        let mut types = entry.kind.iter().filter(|(_, enabled)| **enabled);
        let protocol = match (types.next().map(|(name, _)| name.as_str()), types.next()) {
            (Some("efa"), None) => Protocol::Efa,
            (Some("motis"), None) => Protocol::Motis,
            (Some("trias"), None) => Protocol::Trias,
            (Some("ojp"), None) => Protocol::Ojp1,
            (Some("hafasMgate"), None) => Protocol::HafasMgate,
            // The type covers both APIs of OTP, and only Entur's entry
            // says which one it is.
            (Some("otpGraphQl"), None)
                if options.api_version.as_deref() == Some("entur")
                    || options.endpoint.contains("transmodel") =>
            {
                Protocol::OtpTransmodel
            }
            (Some("otpGraphQl"), None) => {
                return Err(invalid(format!(
                    "{id}: of OpenTripPlanner, only the Transmodel API is supported"
                )));
            }
            _ => {
                return Err(invalid(format!(
                    "{id}: needs exactly one of the types efa, motis, trias, ojp, hafasMgate and otpGraphQl"
                )));
            }
        };
        let hafas = (protocol == Protocol::HafasMgate)
            .then(|| options.hafas(id, entry.supported_languages))
            .transpose()?;
        if protocol == Protocol::Ojp1 {
            // Written as text here, as a number for TRIAS upstream.
            let version = options.ojp_version.as_ref().map(ToString::to_string);
            if !matches!(version.as_deref(), Some("\"1.0\"" | "1.0")) {
                return Err(invalid(format!("{id}: only OJP 1.0 is supported")));
            }
        }
        // The other method is an HTTP header, which a network cannot carry.
        if options
            .authorization_method
            .is_some_and(|method| method != "RequestorRef")
        {
            return Err(invalid(format!(
                "{id}: the server takes its key in a way this crate cannot send"
            )));
        }
        let endpoint = if protocol == Protocol::Motis {
            let root = options.endpoint.trim_end_matches('/');
            if root.ends_with("/api") {
                root.to_owned()
            } else {
                format!("{root}/api")
            }
        } else {
            options.endpoint
        };

        let xml = matches!(protocol, Protocol::Trias | Protocol::Ojp1);
        let unsupported =
            options
                .provided_services
                .filter(|_| xml)
                .map_or_else(Vec::new, |provided| {
                    [
                        ("LocationInformationRequest", Operation::Locations),
                        // The search by position is the same service.
                        ("LocationInformationRequest", Operation::Nearby),
                        ("StopEventRequest", Operation::Departures),
                        ("TripRequest", Operation::Trips),
                    ]
                    .into_iter()
                    .filter(|(service, _)| !provided.iter().any(|name| name == service))
                    .map(|(_, operation)| operation)
                    .collect()
                });

        let coverage = entry.coverage.unwrap_or_default();
        let area = |area: Option<WireArea>| area.map(|area| area.into_area(id)).transpose();
        Ok(Self {
            id: id.to_owned(),
            name: entry.name,
            endpoint,
            protocol,
            timezone: entry.timezone,
            requestor_ref: options.requestor_ref,
            coverage: Coverage {
                realtime: area(coverage.realtime)?,
                regular: area(coverage.regular)?,
                any: area(coverage.any)?,
            },
            attribution: entry.attribution.map(|attribution| Attribution {
                name: attribution.name,
                homepage: attribution.homepage,
                license: attribution.license,
            }),
            unsupported,
            requires_key: xml && options.requires_sign_up,
            terms: entry.usage_terms.filter(|terms| !terms.is_empty()),
            headers: Vec::new(),
            geocoder: options.geocoder.filter(|url| !url.is_empty()),
            language: None,
            hafas,
        })
    }
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    #[serde(rename = "type")]
    kind: BTreeMap<String, bool>,
    coverage: Option<WireCoverage>,
    options: Options,
    timezone: Option<String>,
    attribution: Option<WireAttribution>,
    #[serde(rename = "usageTerms")]
    usage_terms: Option<String>,
    #[serde(rename = "supportedLanguages", default)]
    supported_languages: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    endpoint: String,
    authorization_method: Option<String>,
    ojp_version: Option<serde_json::Value>,
    requestor_ref: Option<String>,
    #[serde(default)]
    requires_sign_up: bool,
    provided_services: Option<Vec<String>>,
    api_version: Option<String>,
    geocoder: Option<String>,
    client: Option<serde_json::Value>,
    auth: Option<serde_json::Value>,
    ext: Option<String>,
    ver: Option<String>,
    version: Option<String>,
    #[serde(default)]
    products: Vec<WireProduct>,
}

impl Options {
    /// What a HAFAS mgate entry says about its server. Takes the values
    /// out of the options.
    fn hafas(&mut self, id: &str, languages: Vec<String>) -> Result<HafasOptions, Error> {
        let invalid = |name: &str| Error::InvalidNetwork {
            message: format!("{id}: needs options.{name}"),
        };
        let object = |value: Option<serde_json::Value>, name: &str| {
            value
                .filter(serde_json::Value::is_object)
                .map(|value| value.to_string())
                .ok_or_else(|| invalid(name))
        };
        Ok(HafasOptions {
            client: object(self.client.take(), "client")?,
            auth: object(self.auth.take(), "auth")?,
            ext: self.ext.take(),
            // A server rejects a request without a version, and newer
            // servers still answer this old one.
            version: self
                .ver
                .take()
                .or_else(|| self.version.take())
                .unwrap_or_else(|| "1.16".to_owned()),
            language: languages
                .into_iter()
                .next()
                .unwrap_or_else(|| "en".to_owned()),
            products: std::mem::take(&mut self.products)
                .into_iter()
                .map(|product| HafasProduct {
                    id: product.id,
                    name: product.name,
                    bitmasks: product.bitmasks,
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
struct WireProduct {
    id: String,
    #[serde(default)]
    name: String,
    bitmasks: Vec<u32>,
}

#[derive(Deserialize, Default)]
struct WireCoverage {
    #[serde(rename = "realtimeCoverage")]
    realtime: Option<WireArea>,
    #[serde(rename = "regularCoverage")]
    regular: Option<WireArea>,
    #[serde(rename = "anyCoverage")]
    any: Option<WireArea>,
}

#[derive(Deserialize)]
struct WireArea {
    #[serde(default)]
    region: Vec<String>,
    area: Option<Geometry>,
}

/// A `GeoJSON` geometry. A ring is a list of positions, and a position is a
/// longitude, a latitude and sometimes more.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum Geometry {
    Polygon {
        coordinates: Vec<Vec<Vec<f64>>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<Vec<f64>>>>,
    },
}

#[derive(Deserialize)]
struct WireAttribution {
    name: String,
    homepage: Option<String>,
    license: Option<String>,
}

impl WireArea {
    fn into_area(self, id: &str) -> Result<Area, Error> {
        let polygon = |rings: Vec<Vec<Vec<f64>>>| {
            let rings = rings
                .into_iter()
                .map(|ring| {
                    ring.into_iter()
                        .map(|position| match position.as_slice() {
                            [longitude, latitude, ..] => Ok(Coordinate {
                                latitude: *latitude,
                                longitude: *longitude,
                            }),
                            _ => Err(Error::InvalidNetwork {
                                message: format!("{id}: a position needs two numbers"),
                            }),
                        })
                        .collect()
                })
                .collect::<Result<_, _>>()?;
            Ok(Polygon { rings })
        };
        let polygons = match self.area {
            None => Vec::new(),
            Some(Geometry::Polygon { coordinates }) => vec![polygon(coordinates)?],
            Some(Geometry::MultiPolygon { coordinates }) => coordinates
                .into_iter()
                .map(polygon)
                .collect::<Result<_, Error>>()?,
        };
        Ok(Area {
            regions: self.region,
            polygons,
        })
    }
}

/// A network that covers a position, see [`Registry::covering`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct CoveringNetwork {
    /// The network.
    pub network: Network,
    /// The best tier of its coverage at the position.
    pub tier: CoverageTier,
}

/// Declares the networks that ship with the crate: the enum that names
/// them and the table of their entries, from one list.
macro_rules! bundled {
    ($($variant:ident => $id:literal, $entry:literal;)*) => {
        /// A network that ships with the library. Its name is its ID,
        /// `vvo-efa`, in the spelling the language has for such names.
        /// Each network was checked against its live server when it was
        /// added, and leaves the list when its server is gone for good.
        // No export of `Debug`: the other languages then print the name as
        // they spell it.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        #[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
        #[cfg_attr(feature = "unstable-wasm", derive(serde::Serialize, serde::Deserialize, tsify::Tsify))]
        pub enum BundledNetwork {
            $(
                // The documentation of a network is its ID. UniFFI cannot
                // read a `concat!` there that would make a sentence of it.
                #[doc = $id]
                #[cfg_attr(feature = "unstable-wasm", serde(rename = $id))]
                $variant,
            )*
        }

        impl BundledNetwork {
            /// Every bundled network, in the order of the registry.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            /// The ID of the network, such as `vvo-efa`.
            #[must_use]
            pub fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)*
                }
            }

            /// The entry of the network in the format of transport-apis.
            fn entry(self) -> &'static str {
                match self {
                    $(Self::$variant => include_str!($entry),)*
                }
            }

            /// Where the entry is, to check it against the ID.
            #[cfg(test)]
            fn entry_path(self) -> &'static str {
                match self {
                    $(Self::$variant => $entry,)*
                }
            }
        }
    };
}

// The entries come from the transport-apis repository, a submodule, where
// it has a usable entry. Those in `data/networks` are waiting to get there,
// or need the fields that only this crate reads.
bundled! {
    VvoEfa => "vvo-efa", "../data/networks/vvo-efa.json";
    VvoTrias => "vvo-trias", "../data/networks/vvo-trias.json";
    VvoOjp => "vvo-ojp", "../data/networks/vvo-ojp.json";
    VrrEfa => "vrr-efa", "../data/networks/vrr-efa.json";
    VrrTrias => "vrr-trias", "../data/networks/vrr-trias.json";
    VrrOjp => "vrr-ojp", "../data/networks/vrr-ojp.json";
    LinzEfa => "linz-efa", "../data/transport-apis/data/at/linz-efa.json";
    BayernfahrplanEfa => "bayernfahrplan-efa", "../data/transport-apis/data/de/bayernfahrplan-efa.json";
    BwegtEfa => "bwegt-efa", "../data/transport-apis/data/de/bwegt-efa.json";
    KvvEfa => "kvv-efa", "../data/transport-apis/data/de/kvv-efa.json";
    MvvEfa => "mvv-efa", "../data/transport-apis/data/de/mvv-efa.json";
    RolphEfa => "rolph-efa", "../data/transport-apis/data/de/rolph-efa.json";
    VgnEfa => "vgn-efa", "../data/transport-apis/data/de/vgn-efa.json";
    VvsEfa => "vvs-efa", "../data/transport-apis/data/de/vvs-efa.json";
    StaEfa => "sta-efa", "../data/transport-apis/data/it/sta-efa.json";
    OebbHafas => "oebb-hafas", "../data/transport-apis/data/at/oebb-hafas-mgate.json";
    OoevvHafas => "ooevv-hafas", "../data/transport-apis/data/at/ooevv-hafas-mgate.json";
    SalzburgHafas => "salzburg-hafas", "../data/transport-apis/data/at/salzburg-hafas-mgate.json";
    StvHafas => "stv-hafas", "../data/transport-apis/data/at/stv-hafas-mgate.json";
    SvvHafas => "svv-hafas", "../data/transport-apis/data/at/svv-hafas-mgate.json";
    VkgHafas => "vkg-hafas", "../data/transport-apis/data/at/vkg-hafas-mgate.json";
    VorHafas => "vor-hafas", "../data/transport-apis/data/at/vor-hafas-mgate.json";
    VvtHafas => "vvt-hafas", "../data/transport-apis/data/at/vvt-hafas-mgate.json";
    VvvHafas => "vvv-hafas", "../data/transport-apis/data/at/vvv-hafas-mgate.json";
    TpgHafas => "tpg-hafas", "../data/transport-apis/data/ch/tpg-hafas-mgate.json";
    AvvHafas => "avv-hafas", "../data/transport-apis/data/de/avv-hafas-mgate.json";
    BvgHafas => "bvg-hafas", "../data/transport-apis/data/de/bvg-hafas-mgate.json";
    DartHafas => "dart-hafas", "../data/transport-apis/data/us/dart-hafas-mgate.json";
    DbBusradarNrwHafas => "db-busradar-nrw-hafas", "../data/transport-apis/data/de/db-busradar-nrw-hafas-mgate.json";
    DbSbahnMuenchenHafas => "db-sbahn-muenchen-hafas", "../data/transport-apis/data/de/db-sbahn-muenchen-hafas-mgate.json";
    DbSmartrblHafas => "db-smartrbl-hafas", "../data/transport-apis/data/de/db-smartrbl-hafas-mgate.json";
    GvhHafas => "gvh-hafas", "../data/transport-apis/data/de/gvh-hafas-mgate.json";
    InvgHafas => "invg-hafas", "../data/transport-apis/data/de/invg-hafas-mgate.json";
    KvbHafas => "kvb-hafas", "../data/transport-apis/data/de/kvb-hafas-mgate.json";
    NahshHafas => "nahsh-hafas", "../data/transport-apis/data/de/nahsh-hafas-mgate.json";
    NasaHafas => "nasa-hafas", "../data/transport-apis/data/de/nasa-hafas-mgate.json";
    NvvHafas => "nvv-hafas", "../data/transport-apis/data/de/nvv-hafas-mgate.json";
    RmvHafas => "rmv-hafas", "../data/transport-apis/data/de/rmv-hafas-mgate.json";
    RsagHafas => "rsag-hafas", "../data/transport-apis/data/de/rsag-hafas-mgate.json";
    SaarvvHafas => "saarvv-hafas", "../data/transport-apis/data/de/saarvv-hafas-mgate.json";
    VbbHafas => "vbb-hafas", "../data/transport-apis/data/de/vbb-hafas-mgate.json";
    VbnHafas => "vbn-hafas", "../data/transport-apis/data/de/vbn-hafas-mgate.json";
    VmtHafas => "vmt-hafas", "../data/transport-apis/data/de/vmt-hafas-mgate.json";
    VosHafas => "vos-hafas", "../data/transport-apis/data/de/vos-hafas-mgate.json";
    VsnHafas => "vsn-hafas", "../data/transport-apis/data/de/vsn-hafas-mgate.json";
    RejseplanenHafas => "rejseplanen-hafas", "../data/transport-apis/data/dk/rejseplanen-hafas-mgate.json";
    IarnrodEireannHafas => "iarnrod-eireann-hafas", "../data/transport-apis/data/ie/iarnrod-eireann-hafas-mgate.json";
    MobiliteitLuHafas => "mobiliteit-lu-hafas", "../data/transport-apis/data/lu/mobiliteit-lu-hafas-mgate.json";
    ResrobotHafas => "resrobot-hafas", "../data/transport-apis/data/se/resrobot-hafas-mgate.json";
    BartHafas => "bart-hafas", "../data/transport-apis/data/us/bart-hafas-mgate.json";
    Entur => "entur", "../data/transport-apis/data/no/entur-otp.json";
    Transitous => "transitous", "../data/transport-apis/data/un/transitous.json";
}

impl BundledNetwork {
    /// The network, as [`Registry::bundled`] has or would have it.
    pub(crate) fn network(self) -> Network {
        #[expect(clippy::expect_used, reason = "the entries are part of the crate")]
        let network =
            Network::from_json(self.id(), self.entry()).expect("a bundled entry can be read");
        corrected(network)
    }
}

impl Network {
    /// The network that ships with the crate under this name.
    ///
    /// # Panics
    ///
    /// If an entry of the crate cannot be read, which its tests rule out.
    #[must_use]
    pub fn bundled(network: BundledNetwork) -> Self {
        network.network()
    }
}

/// The bundled networks of operators that [`Registry::bundled`] has
/// another server of. Their answers hold less than those of the EFA server.
const FURTHER_SERVERS: [BundledNetwork; 4] = [
    BundledNetwork::VvoTrias,
    BundledNetwork::VvoOjp,
    BundledNetwork::VrrTrias,
    BundledNetwork::VrrOjp,
];

/// A set of networks to choose from.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Registry {
    networks: Vec<Network>,
}

/// What the entries of bundled networks in transport-apis lack or have
/// wrong. A line leaves this function when transport-apis has the value.
fn corrected(mut network: Network) -> Network {
    // The address a server has moved to.
    let moved = match network.id.as_str() {
        "mobiliteit-lu-hafas" => Some("https://cdt.hafas.de/gate"),
        // The entry names the data portal of the province.
        "sta-efa" => Some("https://efa.sta.bz.it/apb/"),
        "ooevv-hafas" => Some("https://app.verkehrsauskunft.at/hamm/gate"),
        "salzburg-hafas" => Some("https://verkehrsauskunft.salzburg.gv.at/hamm/gate"),
        "stv-hafas" => Some("https://verkehrsauskunft.verbundlinie.at/hamm/gate"),
        "svv-hafas" => Some("https://fahrplan.salzburg-verkehr.at/hamm/gate"),
        "vkg-hafas" => Some("https://routenplaner.kaerntner-linien.at/hamm/gate"),
        "vor-hafas" => Some("https://anachb.vor.at/hamm/gate"),
        "vvv-hafas" => Some("https://fahrplan.vmobil.at/hamm/gate"),
        _ => None,
    };
    if let Some(endpoint) = moved {
        endpoint.clone_into(&mut network.endpoint);
    }
    match network.id.as_str() {
        // The entries name no time zone.
        "bayernfahrplan-efa" | "kvv-efa" | "vvs-efa" => {
            network.timezone = Some("Europe/Berlin".to_owned());
        }
        // The entry names the time zone of Berlin for a server in Iowa.
        "dart-hafas" => network.timezone = Some("America/Chicago".to_owned()),
        // The format has no field for a geocoder or for usage terms, and
        // the entry names no time zone.
        "entur" => {
            network.geocoder = Some("https://api.entur.io/geocoder/v1".to_owned());
            network.timezone = Some("Europe/Oslo".to_owned());
        }
        "transitous" => network.terms = Some("https://transitous.org/api/".to_owned()),
        _ => {}
    }
    network
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Registry {
    /// The networks that ship with the crate, one for each operator. Each
    /// was checked against its live server when it was added. Where an
    /// operator has servers for several protocols, this is the one with
    /// the most data; [`Network::bundled`] gives the others.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    #[must_use]
    pub fn bundled() -> Self {
        let networks = BundledNetwork::ALL
            .iter()
            .filter(|network| !FURTHER_SERVERS.contains(network))
            .map(|network| network.network())
            .collect();
        Self { networks }
    }

    /// A registry of the given networks, for example the bundled ones
    /// together with an application's own. If two share an ID,
    /// [`Registry::get`] finds the first.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    #[must_use]
    pub fn new(networks: Vec<Network>) -> Self {
        Self { networks }
    }

    /// All networks, in the order they were given.
    #[must_use]
    pub fn networks(&self) -> Vec<Network> {
        self.networks.clone()
    }

    /// The network with the given ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Network> {
        self.networks
            .iter()
            .find(|network| network.id == id)
            .cloned()
    }

    /// The networks with a coverage polygon that contains the position,
    /// see [`Coverage::tier_at`]. Those with the best tier come first, in
    /// the order of the registry within a tier. Coverage that an entry
    /// gives only as region codes is not found.
    #[must_use]
    pub fn covering(&self, position: Coordinate) -> Vec<CoveringNetwork> {
        let mut covering: Vec<_> = self
            .networks
            .iter()
            .filter_map(|network| {
                Some(CoveringNetwork {
                    tier: network.coverage.tier_at(position)?,
                    network: network.clone(),
                })
            })
            .collect();
        covering.sort_by_key(|covering| covering.tier);
        covering
    }
}

#[cfg(test)]
mod tests {
    use super::BundledNetwork;

    /// The macro pairs each ID with a file by hand.
    #[test]
    fn entry_of_a_bundled_network_is_the_file_of_its_id() {
        for network in BundledNetwork::ALL {
            let path = network.entry_path();
            let file = path.rsplit('/').next().unwrap_or(path);
            assert!(file.starts_with(network.id()), "{network:?}: {path}");
        }
    }
}
