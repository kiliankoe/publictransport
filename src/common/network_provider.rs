use chrono::prelude::*;

use common::network_id::NetworkId;
use common::Capability;
use common::query_departure_result::QueryDepartureResult;

trait NetworkProvider {
    fn id() -> NetworkId;
    fn has_capabilities(capabilities: &[Capability]) -> bool;
    fn query_departures(station_id: &str, time: Option<DateTime<Local>>, max_departures: i8) -> Option<QueryDepartureResult>;
}
