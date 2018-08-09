pub struct QueryDepartureResult {
    status: Status,
}

enum Status {
    Ok,
    InvalidStation,
    ServiceDown,
}
