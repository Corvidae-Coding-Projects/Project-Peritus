//! Credential-free, public-HTTPS preparation shared by native jobs and source builds.

use peritus_network::{AddressClass, classify};
use reqwest::{StatusCode, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fmt::Write as _,
    io::Read as _,
    net::{IpAddr, SocketAddr, ToSocketAddrs as _},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use url::{Host, Url};

const MAX_REDIRECTS: usize = 5;
const MAX_DEPENDENCY_BYTES: u64 = 512 * 1024 * 1024;

/// One bounded public-HTTPS object. A digest is mandatory for module dependencies and optional
/// only when a separately pinned source-tree digest is checked after archive extraction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    /// Absolute public HTTPS URL without credentials.
    pub url: String,
    /// Optional exact byte digest. Source archives instead bind an extracted tree digest.
    pub expected_sha256: Option<String>,
    /// Maximum accepted response bytes.
    pub maximum_bytes: u64,
}

/// Exact transfer observations retained by the owning job.
pub struct Completed {
    /// Original normalized URL.
    pub requested_url: String,
    /// Final normalized URL after bounded redirects.
    pub final_url: String,
    /// Revalidated redirect targets in order.
    pub redirects: Vec<String>,
    /// Public socket addresses actually pinned by the client.
    pub resolved_addresses: Vec<String>,
    /// Exact response bytes.
    pub bytes: Vec<u8>,
    /// SHA-256 of the response bytes.
    pub digest: String,
}

/// Preparation failure class; no caller may turn one into a successful proof or installation.
pub enum Failure {
    /// The owner revoked the operation.
    Cancelled,
    /// The input or remote response violated the preparation contract.
    Rejected(String),
    /// DNS, TLS, transport, or local client setup failed.
    Infrastructure(String),
}

/// Validates the bounded URL and optional exact byte identity.
///
/// # Errors
///
/// Returns an error when URL, digest, or storage bounds are invalid.
pub fn validate(specification: &Specification, storage_bytes: u64) -> Result<Url, String> {
    let url = checked_url(&specification.url)?;
    if specification.expected_sha256.as_ref().is_some_and(|digest| !sha256(digest)) {
        return Err("expected_sha256 must be one lowercase SHA-256 identity".into());
    }
    if specification.maximum_bytes == 0
        || specification.maximum_bytes > MAX_DEPENDENCY_BYTES
        || specification.maximum_bytes > storage_bytes
    {
        return Err(
            "maximum_bytes must fit both the 512 MiB preparation ceiling and requested storage limit"
                .into(),
        );
    }
    Ok(url)
}

/// Fetches one validated object without ambient proxy or credential discovery.
///
/// # Errors
///
/// Returns a classified preparation error for invalid input, cancellation, or infrastructure failure.
pub fn fetch(
    specification: &Specification,
    cancellation: &Arc<AtomicBool>,
) -> Result<Completed, Failure> {
    let requested =
        validate(specification, specification.maximum_bytes).map_err(Failure::Rejected)?;
    let mut current = requested.clone();
    let mut redirects = Vec::new();
    let mut observed_addresses = Vec::new();
    for redirect_count in 0..=MAX_REDIRECTS {
        cancelled(cancellation)?;
        let (host, addresses) = resolve_public(&current).map_err(Failure::Rejected)?;
        observed_addresses.extend(addresses.iter().map(ToString::to_string));
        let client = client(&host, &addresses).map_err(Failure::Infrastructure)?;
        let mut response = client
            .get(current.clone())
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .map_err(|error| Failure::Infrastructure(format!("HTTPS request failed: {error}")))?;
        cancelled(cancellation)?;
        if response.status().is_redirection() {
            if redirect_count == MAX_REDIRECTS {
                return Err(Failure::Rejected("dependency exceeded five HTTPS redirects".into()));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| Failure::Rejected("HTTPS redirect has no valid Location".into()))?;
            let next = current
                .join(location)
                .map_err(|_| Failure::Rejected("HTTPS redirect Location is invalid".into()))?;
            current = checked_url(next.as_str()).map_err(Failure::Rejected)?;
            redirects.push(current.to_string());
            continue;
        }
        if response.status() != StatusCode::OK {
            return Err(Failure::Rejected(format!(
                "dependency origin returned HTTP {}",
                response.status().as_u16()
            )));
        }
        if response.content_length().is_some_and(|bytes| bytes > specification.maximum_bytes) {
            return Err(Failure::Rejected(
                "declared dependency exceeds maximum_bytes before transfer".into(),
            ));
        }
        let capacity = usize::try_from(specification.maximum_bytes.min(1024 * 1024))
            .map_err(|_| Failure::Rejected("dependency size exceeds this platform".into()))?;
        let mut bytes = Vec::with_capacity(capacity);
        let mut chunk = [0_u8; 16 * 1024];
        loop {
            cancelled(cancellation)?;
            let count = response.read(&mut chunk).map_err(|error| {
                Failure::Infrastructure(format!("dependency transfer failed: {error}"))
            })?;
            if count == 0 {
                break;
            }
            let next = u64::try_from(bytes.len())
                .ok()
                .and_then(|current| current.checked_add(u64::try_from(count).ok()?))
                .ok_or_else(|| Failure::Rejected("dependency byte count overflowed".into()))?;
            if next > specification.maximum_bytes {
                return Err(Failure::Rejected(
                    "declared dependency exceeded maximum_bytes during transfer".into(),
                ));
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let digest = hex(&Sha256::digest(&bytes));
        if specification.expected_sha256.as_ref().is_some_and(|expected| expected != &digest) {
            return Err(Failure::Rejected(format!(
                "dependency SHA-256 mismatch: expected {}, observed {digest}",
                specification.expected_sha256.as_deref().unwrap_or_default()
            )));
        }
        observed_addresses.sort();
        observed_addresses.dedup();
        return Ok(Completed {
            requested_url: requested.to_string(),
            final_url: current.to_string(),
            redirects,
            resolved_addresses: observed_addresses,
            bytes,
            digest,
        });
    }
    Err(Failure::Rejected("dependency redirect state is invalid".into()))
}

fn checked_url(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|_| "dependency URL must be absolute HTTPS")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some_and(|query| query.len() > 4096)
        || url.port_or_known_default() != Some(443)
        || !matches!(url.host(), Some(Host::Domain(_)))
    {
        return Err(
            "dependency URL must use an ordinary DNS HTTPS origin on port 443 without credentials"
                .into(),
        );
    }
    url.set_fragment(None);
    Ok(url)
}

fn resolve_public(url: &Url) -> Result<(String, Vec<SocketAddr>), String> {
    let host = url.host_str().ok_or("dependency URL has no DNS host")?.to_ascii_lowercase();
    let mut addresses = (host.as_str(), 443)
        .to_socket_addrs()
        .map_err(|error| format!("dependency DNS resolution failed: {error}"))?
        .collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        return Err("dependency DNS resolution returned no addresses".into());
    }
    if let Some(address) = addresses
        .iter()
        .map(SocketAddr::ip)
        .find(|address| address_class(*address) != AddressClass::Global)
    {
        return Err(format!("dependency DNS returned forbidden non-public address {address}"));
    }
    Ok((host, addresses))
}

fn address_class(address: IpAddr) -> AddressClass {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map_or_else(|| classify(IpAddr::V6(address)), |mapped| classify(IpAddr::V4(mapped))),
        address @ IpAddr::V4(_) => classify(address),
    }
}

fn client(host: &str, addresses: &[SocketAddr]) -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("Peritus/", env!("CARGO_PKG_VERSION"), " dependency-preparation"))
        .https_only(true)
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(2))
        .resolve_to_addrs(host, addresses)
        .build()
        .map_err(|error| format!("dependency HTTPS client could not start: {error}"))
}

fn cancelled(cancellation: &Arc<AtomicBool>) -> Result<(), Failure> {
    if cancellation.load(Ordering::Acquire) { Err(Failure::Cancelled) } else { Ok(()) }
}

fn sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}
