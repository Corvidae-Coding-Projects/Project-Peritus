//! Bounded redirect-chain revalidation.

use peritus_sandbox::{DnsName, NetworkHost, Transport};

use crate::{DestinationRequest, NetworkError, NetworkPlan, RedirectMode};

/// One parsed absolute HTTP redirect target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedirectTarget {
    request: DestinationRequest,
    path_and_query: String,
}

impl RedirectTarget {
    /// Parses an absolute `http://` or `https://` URI without accepting user-info or fragments.
    ///
    /// # Errors
    /// Rejects malformed, oversized, non-HTTP, or ambiguous authority.
    pub fn parse(value: &str) -> Result<Self, NetworkError> {
        if value.len() > 8_192 || value.contains('#') || value.contains('@') {
            return Err(redirect_error("redirect URI is malformed or exceeds its bound"));
        }
        let (remainder, default_port) = if let Some(rest) = value.strip_prefix("http://") {
            (rest, 80)
        } else {
            return Err(redirect_error(
                "redirect URI is not plain HTTP and cannot be followed by this proxy",
            ));
        };
        let split = remainder.find('/').unwrap_or(remainder.len());
        let authority = &remainder[..split];
        let path = if split == remainder.len() { "/" } else { &remainder[split..] };
        let (host, port) = parse_authority(authority, default_port)?;
        let host = match host.parse() {
            Ok(address) => NetworkHost::Ip(address),
            Err(_) => NetworkHost::Dns(
                DnsName::new(host).map_err(|_| redirect_error("redirect host is invalid"))?,
            ),
        };
        let request = DestinationRequest::new(host, Transport::Tcp, port)?;
        Ok(Self { request, path_and_query: path.to_owned() })
    }
    /// Creates a same-authority successor for one origin-form location.
    ///
    /// # Errors
    /// Rejects an empty, oversized, or non-origin-form path.
    pub fn relative(
        request: DestinationRequest,
        path_and_query: &str,
    ) -> Result<Self, NetworkError> {
        if !path_and_query.starts_with('/') || path_and_query.len() > 8_192 {
            return Err(redirect_error("relative redirect path is malformed or exceeds its bound"));
        }
        Ok(Self { request, path_and_query: path_and_query.to_owned() })
    }
    /// Returns the destination request.
    #[must_use]
    pub const fn request(&self) -> &DestinationRequest {
        &self.request
    }
    /// Returns the origin-form path and query.
    #[must_use]
    pub fn path_and_query(&self) -> &str {
        &self.path_and_query
    }
}

/// Ordered redirect state for one request.
#[derive(Clone, Debug)]
pub struct RedirectChain<'a> {
    plan: &'a NetworkPlan,
    depth: u8,
}

impl<'a> RedirectChain<'a> {
    /// Creates an empty redirect chain.
    #[must_use]
    pub const fn new(plan: &'a NetworkPlan) -> Self {
        Self { plan, depth: 0 }
    }
    /// Re-evaluates one successor and advances the count.
    ///
    /// # Errors
    /// Rejects redirects disabled by plan, a crossed count, or a denied successor.
    pub fn follow(&mut self, target: RedirectTarget) -> Result<RedirectTarget, NetworkError> {
        let maximum = match self.plan.options().redirects() {
            RedirectMode::Deny => return Err(redirect_error("redirects are disabled")),
            RedirectMode::Follow { maximum } => maximum,
        };
        let next =
            self.depth.checked_add(1).ok_or_else(|| redirect_error("redirect count overflowed"))?;
        if next > maximum {
            return Err(redirect_error("redirect count exceeds its bound"));
        }
        if self.plan.decide_request(target.request())? != crate::DestinationDecision::Allowed {
            return Err(redirect_error("redirect successor is outside checked authority"));
        }
        self.depth = next;
        Ok(target)
    }
    /// Returns current depth.
    #[must_use]
    pub const fn depth(&self) -> u8 {
        self.depth
    }
}

fn parse_authority(authority: &str, default_port: u16) -> Result<(&str, u16), NetworkError> {
    if authority.is_empty() {
        return Err(redirect_error("redirect authority is empty or unsupported"));
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let close =
            rest.find(']').ok_or_else(|| redirect_error("redirect IPv6 authority is malformed"))?;
        let host = &rest[..close];
        let suffix = &rest[close + 1..];
        let port = if suffix.is_empty() {
            default_port
        } else {
            suffix
                .strip_prefix(':')
                .ok_or_else(|| redirect_error("redirect IPv6 authority is malformed"))?
                .parse::<u16>()
                .map_err(|_| redirect_error("redirect port is invalid"))?
        };
        if port == 0 {
            return Err(redirect_error("redirect port is zero"));
        }
        return Ok((host, port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port =
                port.parse::<u16>().map_err(|_| redirect_error("redirect port is invalid"))?;
            if port == 0 {
                return Err(redirect_error("redirect port is zero"));
            }
            Ok((host, port))
        }
        _ => Ok((authority, default_port)),
    }
}

const fn redirect_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        crate::NetworkErrorKind::Redirect,
        crate::NetworkOperation::Redirect,
        crate::RecoveryClass::Replan,
        detail,
    )
}
