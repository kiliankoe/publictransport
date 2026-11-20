use crate::Error;

/// An HTTP request for the caller to send.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct HttpRequest {
    /// The request method.
    pub method: Method,
    /// The absolute URL, including the query string.
    pub url: String,
    /// Headers the server needs. The caller may add its own.
    pub headers: Vec<Header>,
    /// The request body, if the method has one.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes"),
        tsify(type = "Uint8Array")
    )]
    pub body: Option<Vec<u8>>,
}

/// An HTTP request method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "UPPERCASE")
)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
}

/// An HTTP response as the caller received it. The HTTP client must not
/// follow redirects, so that a moved endpoint is reported as [`Error::Moved`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// All response headers. Names are matched case-insensitively.
    pub headers: Vec<Header>,
    /// The raw response body.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(with = "serde_bytes"),
        tsify(type = "Uint8Array")
    )]
    pub body: Vec<u8>,
}

/// One HTTP header. Its debug output leaves the value out, because that can
/// be a key and debug output ends up in logs.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug, Display))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Header {
    /// The header name.
    pub name: String,
    /// The header value.
    pub value: String,
}

impl std::fmt::Debug for Header {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Header")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The same as the debug output, so that no way of printing a header in
/// another language shows the value.
impl std::fmt::Display for Header {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

impl HttpRequest {
    /// A `GET` request for `path` below `endpoint` with the given query
    /// parameters.
    pub(crate) fn get<'a>(
        endpoint: &str,
        path: &str,
        parameters: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let query = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(parameters)
            .finish();
        Self {
            method: Method::Get,
            url: format!("{}/{path}?{query}", endpoint.trim_end_matches('/')),
            headers: Vec::new(),
            body: None,
        }
    }
}

impl HttpResponse {
    /// The body of a successful response, or the error its status stands for.
    pub(crate) fn success_body(&self) -> Result<&[u8], Error> {
        match self.status {
            200..=299 => Ok(&self.body),
            301 | 302 | 303 | 307 | 308 => Err(Error::Moved {
                location: self.header("location").map(str::to_owned),
            }),
            401 => Err(Error::Unauthorized),
            403 => Err(Error::Blocked),
            429 => Err(Error::RateLimited),
            500..=599 => Err(Error::Unavailable),
            status => Err(Error::Server {
                status,
                message: String::from_utf8_lossy(&self.body)
                    .chars()
                    .take(200)
                    .collect(),
            }),
        }
    }

    /// Parses the body of a successful response as JSON.
    pub(crate) fn json<'a, T: serde::Deserialize<'a>>(&'a self) -> Result<T, Error> {
        serde_json::from_slice(self.success_body()?).map_err(|error| Error::Parse {
            message: error.to_string(),
        })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }
}
