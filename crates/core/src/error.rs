/// Why a request could not be built or a response could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The server redirected the request, so the endpoint has moved.
    #[error("the endpoint has moved (new location: {location:?})")]
    Moved {
        /// The value of the `Location` header.
        location: Option<String>,
    },
    /// The server refuses this client (status 403). Trying again does not
    /// help.
    #[error("the server refused the request")]
    Blocked,
    /// The client sent too many requests (status 429). Trying again later
    /// can help.
    #[error("the server received too many requests")]
    RateLimited,
    /// The server rejected the credentials (status 401).
    #[error("the server rejected the credentials")]
    Unauthorized,
    /// The server failed (status 5xx).
    #[error("the server is unavailable")]
    Unavailable,
    /// The server answered with any other failure.
    #[error("the server answered with status {status}: {message}")]
    Server {
        /// The HTTP status code.
        status: u16,
        /// The start of the response body.
        message: String,
    },
    /// The response was not in the format the protocol defines.
    #[error("the response could not be read: {message}")]
    Parse {
        /// What was wrong with the response.
        message: String,
    },
}
