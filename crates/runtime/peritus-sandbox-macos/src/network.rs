//! Managed-proxy-only network projection.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use peritus_process::NativeProtectedHandle;

use crate::{
    MacosError, MacosOperation,
    canonical::{Reader, Writer},
    error,
};

const MAX_HANDLE_LABEL_BYTES: usize = 256;

/// Nonsensitive proxy endpoint and inherited-handle metadata encoded in the helper manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyHandleDescriptor {
    route: ProxyRoute,
    label: String,
    payload_len: u32,
}

impl ProxyHandleDescriptor {
    pub(crate) fn new(
        route: ProxyRoute,
        label: String,
        payload_len: u32,
    ) -> Result<Self, MacosError> {
        if label.is_empty()
            || label.len() > MAX_HANDLE_LABEL_BYTES
            || !label.is_ascii()
            || label.bytes().any(|byte| byte.is_ascii_control())
            || payload_len != 32
        {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "proxy handle metadata is invalid or incomplete",
            ));
        }
        Ok(Self { route, label, payload_len })
    }

    /// Returns the exact managed proxy route.
    #[must_use]
    pub const fn route(&self) -> ProxyRoute {
        self.route
    }

    /// Returns the nonsensitive protected-handle label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the exact bounded routing-token payload length.
    #[must_use]
    pub const fn payload_len(&self) -> u32 {
        self.payload_len
    }

    pub(crate) fn encode(&self, writer: &mut Writer) -> Result<(), MacosError> {
        match self.route.endpoint().ip() {
            IpAddr::V4(address) => {
                writer.u8(4)?;
                writer.fixed(&address.octets())?;
            }
            IpAddr::V6(address) => {
                writer.u8(6)?;
                writer.fixed(&address.octets())?;
            }
        }
        writer.u16(self.route.endpoint().port())?;
        writer.u32(self.route.routing_handle())?;
        writer.string(&self.label)?;
        writer.u32(self.payload_len)
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        let address = match reader.u8()? {
            4 => IpAddr::V4(Ipv4Addr::from(reader.fixed::<4>()?)),
            6 => IpAddr::V6(Ipv6Addr::from(reader.fixed::<16>()?)),
            _ => {
                return Err(error::invalid(MacosOperation::Manifest, "invalid proxy address tag"));
            }
        };
        let route = ProxyRoute::new(SocketAddr::new(address, reader.u16()?), reader.u32()?)?;
        Self::new(route, reader.string()?, reader.u32()?)
    }
}

/// Managed proxy route paired with its process-owned opaque routing-token payload.
#[derive(Clone, Debug)]
pub struct ProtectedProxyRoute {
    route: ProxyRoute,
    handle: NativeProtectedHandle,
}

impl ProtectedProxyRoute {
    /// Binds an exact loopback endpoint to one protected proxy-routing token handle.
    ///
    /// # Errors
    /// Rejects an invalid endpoint or a native handle outside the macOS descriptor range.
    pub fn new(endpoint: SocketAddr, handle: NativeProtectedHandle) -> Result<Self, MacosError> {
        if handle.payload_len() != Some(32) {
            return Err(error::invalid(
                MacosOperation::Validate,
                "proxy routing token payload must contain exactly 32 bytes",
            ));
        }
        let descriptor = u32::try_from(handle.raw_handle()).map_err(|_| {
            error::invalid(
                MacosOperation::Validate,
                "proxy routing handle is outside macOS descriptor range",
            )
        })?;
        if descriptor > i32::MAX.cast_unsigned() {
            return Err(error::invalid(
                MacosOperation::Validate,
                "proxy routing handle exceeds native descriptor range",
            ));
        }
        Ok(Self { route: ProxyRoute::new(endpoint, descriptor)?, handle })
    }

    /// Returns the nonsensitive endpoint/descriptor projection encoded in the manifest.
    #[must_use]
    pub const fn route(&self) -> ProxyRoute {
        self.route
    }

    /// Returns the protected proxy-routing token payload.
    #[must_use]
    pub const fn handle(&self) -> &NativeProtectedHandle {
        &self.handle
    }

    pub(crate) fn descriptor(&self) -> Result<ProxyHandleDescriptor, MacosError> {
        ProxyHandleDescriptor::new(
            self.route,
            self.handle.label().to_owned(),
            u32::try_from(self.handle.payload_len().ok_or_else(|| {
                error::invalid(
                    MacosOperation::Validate,
                    "proxy token handle has no finite payload length",
                )
            })?)
            .map_err(|_| {
                error::invalid(MacosOperation::Validate, "proxy token payload is too large")
            })?,
        )
    }
}

/// One exact loopback proxy route available to a prepared target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProxyRoute {
    endpoint: SocketAddr,
    routing_handle: u32,
}

impl ProxyRoute {
    /// Validates a loopback TCP proxy endpoint and an inherited opaque-routing-token handle.
    ///
    /// # Errors
    /// Returns an error for a non-loopback endpoint, port zero, or reserved standard descriptor.
    pub fn new(endpoint: SocketAddr, routing_handle: u32) -> Result<Self, MacosError> {
        if !endpoint.ip().is_loopback() || endpoint.port() == 0 {
            return Err(error::invalid(
                MacosOperation::Validate,
                "managed proxy endpoint must be nonzero loopback",
            ));
        }
        if routing_handle < 3 {
            return Err(error::invalid(
                MacosOperation::Validate,
                "proxy routing handle overlaps a standard descriptor",
            ));
        }
        Ok(Self { endpoint, routing_handle })
    }

    /// Returns the exact loopback endpoint.
    #[must_use]
    pub const fn endpoint(self) -> SocketAddr {
        self.endpoint
    }

    /// Returns the inherited handle containing the opaque routing token.
    #[must_use]
    pub const fn routing_handle(self) -> u32 {
        self.routing_handle
    }

    pub(crate) fn seatbelt_remote(self) -> String {
        match self.endpoint.ip() {
            IpAddr::V4(address) => format!("{address}:{}", self.endpoint.port()),
            IpAddr::V6(address) => format!("[{address}]:{}", self.endpoint.port()),
        }
    }
}
