mod network_provider;
pub mod network_id;
pub mod query_departure_result;

pub enum Capability {
    /// can suggest locations
    SuggestLocations,
    /// can determine nearby locations
    NearbyLocations,
    /// can query for departures
    Departures,
    /// can query for trips
    Trips,
}

pub enum Optimize {
    LeastDuration,
    LeastChanges,
    LeastWalking,
}

pub enum WalkingSpeed {
    Slow,
    Normal,
    Fast,
}

pub enum Accessibility {
    Neutral,
    Limited,
    BarrierFree,
}

pub enum Option {
    Bike,
}
