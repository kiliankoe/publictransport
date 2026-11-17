/// Why a request could not be built or a response could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
pub enum Error {
    /// The [`Network`](crate::Network) lacks something the request needs,
    /// such as a time zone, or its description cannot be read.
    #[error("the network is not usable: {message}")]
    InvalidNetwork {
        /// What is missing or wrong.
        message: String,
    },
    /// The query holds a value no request of the protocol can carry, such as
    /// a time outside the supported range.
    #[error("the query is invalid: {message}")]
    InvalidQuery {
        /// What is wrong.
        message: String,
    },
    /// The network's server is known not to answer this kind of request,
    /// see [`Network::unsupported`](crate::Network::unsupported).
    #[error("not supported: {message}")]
    Unsupported {
        /// What is not supported.
        message: String,
    },
    /// The network does not know the stop the query names, or cannot plan
    /// a trip from or to a place because it knows no stop near it.
    #[error("not found: {message}")]
    NotFound {
        /// What the server said, or what was missing in its answer.
        message: String,
    },
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
    /// The server rejected the credentials (status 401, or the code `AUTH`
    /// in a HAFAS answer).
    #[error("the server rejected the credentials")]
    Unauthorized,
    /// The server failed (status 5xx).
    #[error("the server is unavailable")]
    Unavailable,
    /// The server reported any other failure: an HTTP status that no other
    /// variant covers, or an error inside a successful response, as TRIAS,
    /// OJP and HAFAS send them.
    #[error("the server reported an error (HTTP status {status}): {message}")]
    Server {
        /// The HTTP status code. It is 200 for an error inside a successful
        /// response.
        status: u16,
        /// The start of the response body, or the error code or text the
        /// server put into a successful response.
        message: String,
    },
    /// The response was not in the format the protocol defines.
    #[error("the response could not be read: {message}")]
    Parse {
        /// What was wrong with the response.
        message: String,
    },
}
