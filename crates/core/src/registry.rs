//! Networks as data: entries in the format of the `transport-apis`
//! repository, and the set that ships with the crate.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{Coordinate, Error, Network, Protocol};

/// One of the three things a [`Network`] can be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub enum Operation {
    /// A search for locations.
    Locations,
    /// The departures at a stop.
    Departures,
    /// Trips between two places.
    Trips,
}

/// Where a [`Network`] has data, in three tiers of quality.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct Coverage {
    /// Where the network has real-time data.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub realtime: Option<Area>,
    /// Where it has complete timetables.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub regular: Option<Area>,
    /// Where it has any data.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub any: Option<Area>,
}

/// A part of the world, described by region codes, by polygons or by both.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct Area {
    /// ISO 3166 codes of countries (`DE`) or their subdivisions (`DE-SN`).
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub regions: Vec<String>,
    /// The outlines of the area.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub polygons: Vec<Polygon>,
}

/// A polygon as in `GeoJSON`: the first ring is the outline, further rings
/// are holes in it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct Polygon {
    /// Each ring is a closed line of positions.
    pub rings: Vec<Vec<Coordinate>>,
}

/// A tier of a [`Coverage`], best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
pub struct Attribution {
    /// The name of the source.
    pub name: String,
    /// Its website.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub homepage: Option<String>,
    /// The license of the data, where the source has a single one.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
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
    /// The crate reads three fields that repository does not have: the type
    /// `ojp` with `options.ojpVersion`, and `options.requestorRef`. For
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
        let options = entry.options;

        let mut types = entry.kind.iter().filter(|(_, enabled)| **enabled);
        let protocol = match (types.next().map(|(name, _)| name.as_str()), types.next()) {
            (Some("efa"), None) => Protocol::Efa,
            (Some("motis"), None) => Protocol::Motis,
            (Some("trias"), None) => Protocol::Trias,
            (Some("ojp"), None) => Protocol::Ojp1,
            _ => {
                return Err(invalid(format!(
                    "{id}: needs exactly one of the types efa, motis, trias and ojp"
                )));
            }
        };
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
pub struct CoveringNetwork {
    /// The network.
    pub network: Network,
    /// The best tier of its coverage at the position.
    pub tier: CoverageTier,
}

/// The entries that ship with the crate, by ID. They come from the
/// transport-apis repository, a submodule, where it has a usable entry.
/// Those in `data/networks` are waiting to get there, or need the fields
/// that only this crate reads. They come from the
/// transport-apis repository, a submodule, where it has a usable entry.
/// Those in `data/networks` are waiting to get there, or need the fields
/// that only this crate reads.
const BUNDLED: [(&str, &str); 7] = [
    ("vvo-efa", include_str!("../data/networks/vvo-efa.json")),
    ("vvo-trias", include_str!("../data/networks/vvo-trias.json")),
    ("vvo-ojp", include_str!("../data/networks/vvo-ojp.json")),
    ("vrr-efa", include_str!("../data/networks/vrr-efa.json")),
    ("vrr-trias", include_str!("../data/networks/vrr-trias.json")),
    ("vrr-ojp", include_str!("../data/networks/vrr-ojp.json")),
    (
        "transitous",
        include_str!("../data/transport-apis/data/un/transitous.json"),
    ),
];

/// A set of networks to choose from.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Registry {
    networks: Vec<Network>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Registry {
    /// The networks that ship with the crate. Each was checked against its
    /// live server when it was added.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    #[must_use]
    pub fn bundled() -> Self {
        // The tests read every entry, so none is skipped here.
        let networks = BUNDLED
            .iter()
            .filter_map(|(id, json)| Network::from_json(id, json).ok())
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
