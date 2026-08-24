//! Default Reqwest/Rustls transport with ambient proxies, redirects, and implicit retries disabled.

use core::fmt;
use core::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;

use crate::{
    BoxFuture, ByteStream, CancellationToken, Header, HeaderName, HeaderValue, HttpHeaders,
    HttpLimits, HttpMethod, HttpRequest, HttpResponse, HttpTransport, ProviderCoreError,
    ProviderCoreErrorKind, StatusCode,
};

/// Production HTTP transport backed privately by Reqwest and Rustls.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    limits: HttpLimits,
}

impl ReqwestTransport {
    /// Builds a transport with explicit resource limits.
    ///
    /// Redirects, ambient proxies, automatic referer forwarding, and Reqwest's implicit protocol
    /// retries are disabled. Platform root certificates and Rustls certificate verification remain
    /// enabled.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration failure if the client cannot be built.
    pub fn new(limits: HttpLimits) -> Result<Self, ProviderCoreError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .referer(false)
            .tls_backend_rustls()
            .build()
            .map_err(|_| {
                ProviderCoreError::new(
                    ProviderCoreErrorKind::Configuration,
                    "http_client",
                    "Reqwest/Rustls client construction failed",
                )
            })?;
        Ok(Self { client, limits })
    }

    /// Builds a transport with production limits.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration failure if the client cannot be built.
    pub fn production() -> Result<Self, ProviderCoreError> {
        Self::new(HttpLimits::PRODUCTION)
    }

    /// Returns the configured resource limits.
    #[must_use]
    pub const fn limits(&self) -> HttpLimits {
        self.limits
    }
}

impl fmt::Debug for ReqwestTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReqwestTransport")
            .field("client", &"[private Reqwest/Rustls client]")
            .field("limits", &self.limits)
            .finish()
    }
}

impl HttpTransport for ReqwestTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ProviderCoreError::cancelled("send_request"));
            }
            let (method, endpoint, headers, body) = request.into_parts();
            let mut request = self.client.request(reqwest_method(method), endpoint.url().clone());
            for header in &headers {
                let name =
                    reqwest::header::HeaderName::from_bytes(header.name().as_str().as_bytes())
                        .map_err(|_| invalid_http("validated request header name was rejected"))?;
                let mut value =
                    reqwest::header::HeaderValue::from_bytes(header.value().wire_bytes())
                        .map_err(|_| invalid_http("validated request header value was rejected"))?;
                value.set_sensitive(header.value().is_sensitive());
                request = request.header(name, value);
            }
            request = request.body(body);
            let response = crate::cancellation::first(cancellation, request.send())
                .await
                .ok_or_else(|| ProviderCoreError::cancelled("send_request"))?
                .map_err(|error| map_reqwest_send_error(&error))?;
            if response
                .content_length()
                .is_some_and(|length| length > self.limits.max_response_body_bytes() as u64)
            {
                return Err(limit_error("response content length exceeds its byte bound"));
            }
            let status = StatusCode::new(response.status().as_u16())?;
            let headers = response_headers(response.headers(), self.limits)?;
            let stream = ReqwestByteStream {
                stream: Box::pin(response.bytes_stream()),
                max_chunk_bytes: self.limits.max_chunk_bytes(),
                max_response_body_bytes: self.limits.max_response_body_bytes(),
                observed_bytes: 0,
            };
            HttpResponse::new(status, headers, Box::new(stream), self.limits)
        })
    }
}

struct ReqwestByteStream {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    max_chunk_bytes: usize,
    max_response_body_bytes: usize,
    observed_bytes: usize,
}

impl ByteStream for ReqwestByteStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, ProviderCoreError>> {
        Box::pin(async move {
            match crate::cancellation::first(cancellation, self.stream.next()).await {
                None => Err(ProviderCoreError::cancelled("read_body")),
                Some(item) => match item {
                    Some(Ok(bytes)) => {
                        if bytes.len() > self.max_chunk_bytes {
                            return Err(ProviderCoreError::new(
                                ProviderCoreErrorKind::LimitExceeded,
                                "read_body",
                                "upstream response chunk exceeds its byte bound",
                            ));
                        }
                        self.observed_bytes =
                            self.observed_bytes.checked_add(bytes.len()).ok_or_else(|| {
                                ProviderCoreError::new(
                                    ProviderCoreErrorKind::LimitExceeded,
                                    "read_body",
                                    "upstream response byte count overflowed",
                                )
                            })?;
                        if self.observed_bytes > self.max_response_body_bytes {
                            return Err(ProviderCoreError::new(
                                ProviderCoreErrorKind::LimitExceeded,
                                "read_body",
                                "upstream response body exceeds its cumulative byte bound",
                            ));
                        }
                        Ok(Some(bytes.to_vec()))
                    }
                    Some(Err(_)) => Err(ProviderCoreError::new(
                        ProviderCoreErrorKind::Transport,
                        "read_body",
                        "response body transport failed",
                    )),
                    None => Ok(None),
                },
            }
        })
    }
}

fn response_headers(
    headers: &reqwest::header::HeaderMap,
    limits: HttpLimits,
) -> Result<HttpHeaders, ProviderCoreError> {
    if headers.len() > limits.max_headers() {
        return Err(limit_error("response header count exceeds its bound"));
    }
    let mut result = Vec::with_capacity(headers.len());
    let mut byte_count = 0_usize;
    for (name, value) in headers {
        byte_count = byte_count
            .checked_add(name.as_str().len())
            .and_then(|count| count.checked_add(value.as_bytes().len()))
            .ok_or_else(|| limit_error("response header byte count overflowed"))?;
        if byte_count > limits.max_header_bytes() {
            return Err(limit_error("response headers exceed their cumulative byte bound"));
        }
        let name = HeaderName::new(name.as_str().to_owned())?;
        let sensitive = value.is_sensitive()
            || matches!(name.as_str(), "set-cookie" | "www-authenticate" | "proxy-authenticate");
        let value = if sensitive {
            HeaderValue::sensitive(value.as_bytes().to_vec())?
        } else {
            HeaderValue::new(value.as_bytes().to_vec())?
        };
        result.push(Header::from_value(name, value));
    }
    HttpHeaders::new(result, limits)
}

const fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}

fn map_reqwest_send_error(error: &reqwest::Error) -> ProviderCoreError {
    if error.is_connect() {
        return ProviderCoreError::connect(
            "send_request",
            "connection or TLS establishment failed before submission",
        );
    }
    let detail = if error.is_timeout() {
        "request transport timed out"
    } else if error.is_request() {
        "request transport rejected the request"
    } else {
        "request transport failed"
    };
    ProviderCoreError::new(ProviderCoreErrorKind::Transport, "send_request", detail)
}

const fn invalid_http(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::InvalidHttp, "send_request", detail)
}

const fn limit_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::LimitExceeded, "send_request", detail)
}
