//! Exact packaged G0 local-transport expectations.

use sha2::{Digest as _, Sha256};

use crate::{
    InstallPath, Platform, QualificationError, QualificationErrorCode, QualificationRecovery,
};

/// Validated nonzero C0 store identity used to derive a stable G0 endpoint name.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StoreIdentity([u8; 16]);

impl StoreIdentity {
    /// Creates a nonzero store identity.
    ///
    /// # Errors
    ///
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, QualificationError> {
        if bytes == [0; 16] {
            return Err(transport_error("daemon store identity must be nonzero"));
        }
        Ok(Self(bytes))
    }

    /// Parses the strict 32-hexadecimal-digit G0 configuration representation.
    ///
    /// # Errors
    ///
    /// Rejects malformed or zero identities.
    pub fn from_hex(value: &str) -> Result<Self, QualificationError> {
        if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(transport_error(
                "daemon store identity must contain 32 hexadecimal digits",
            ));
        }
        let mut bytes = [0_u8; 16];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        Self::new(bytes)
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Derives the exact non-secret `peritus-<32-hex>` daemon endpoint name used by G0.
    #[must_use]
    pub fn endpoint_name(self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/daemon-endpoint/v1\0");
        hasher.update(self.0);
        let digest: [u8; 32] = hasher.finalize().into();
        let mut output = String::with_capacity(40);
        output.push_str("peritus-");
        for byte in &digest[..16] {
            use core::fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        }
        output
    }
}

/// Target-native endpoint address supplied to G1 and G2.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EndpointAddress {
    /// Protected Unix-domain socket path.
    Unix(InstallPath),
    /// Owner-restricted local Windows named-pipe name.
    WindowsNamedPipe(String),
}

impl EndpointAddress {
    /// Returns the exact argument spelling accepted by `peritus --endpoint` and
    /// `peritus-tui --endpoint`.
    #[must_use]
    pub fn as_argument(&self) -> &str {
        match self {
            Self::Unix(path) => path.as_str(),
            Self::WindowsNamedPipe(name) => name,
        }
    }
}

/// Complete H2 expectation for the G0 endpoint published by a package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointExpectation {
    address: EndpointAddress,
    instance_record: InstallPath,
    same_user_only: bool,
    unix_socket_mode: Option<u16>,
    remote_listener_permitted: bool,
}

impl EndpointExpectation {
    /// Derives the endpoint and instance record from the same state root and store identity as G0.
    ///
    /// # Errors
    ///
    /// Returns a layout error if the Unix endpoint path cannot be represented canonically.
    pub fn derive(
        platform: Platform,
        state_root: &InstallPath,
        store: StoreIdentity,
    ) -> Result<Self, QualificationError> {
        let endpoint_name = store.endpoint_name();
        let address = match platform {
            Platform::Linux | Platform::Macos => {
                EndpointAddress::Unix(state_root.join(platform, &format!("{endpoint_name}.sock"))?)
            }
            Platform::Windows => {
                EndpointAddress::WindowsNamedPipe(format!(r"\\.\pipe\{endpoint_name}"))
            }
        };
        Ok(Self {
            address,
            instance_record: state_root.join(platform, "daemon.instance")?,
            same_user_only: true,
            unix_socket_mode: (platform != Platform::Windows).then_some(0o600),
            remote_listener_permitted: false,
        })
    }

    /// Borrows the target-native address.
    #[must_use]
    pub const fn address(&self) -> &EndpointAddress {
        &self.address
    }

    /// Borrows the ephemeral live-instance record containing the endpoint name and process birth
    /// identity.
    #[must_use]
    pub const fn instance_record(&self) -> &InstallPath {
        &self.instance_record
    }

    /// Reports whether the operating-system peer must be the daemon's owning user.
    #[must_use]
    pub const fn same_user_only(&self) -> bool {
        self.same_user_only
    }

    /// Returns the exact Unix socket mode, or `None` for a Windows named pipe.
    #[must_use]
    pub const fn unix_socket_mode(&self) -> Option<u16> {
        self.unix_socket_mode
    }

    /// Reports whether any TCP or remote listener is allowed.
    #[must_use]
    pub const fn remote_listener_permitted(&self) -> bool {
        self.remote_listener_permitted
    }

    /// Returns the exact two arguments required to direct a packaged G1 or G2 client to G0.
    #[must_use]
    pub fn client_arguments(&self) -> [String; 2] {
        ["--endpoint".to_owned(), self.address.as_argument().to_owned()]
    }
}

const fn nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}

fn transport_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::InvalidInput,
        QualificationRecovery::CorrectInput,
        "validate packaged daemon transport",
        detail,
    )
}
