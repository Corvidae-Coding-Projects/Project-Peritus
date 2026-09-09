//! Stable non-secret daemon instance identity.

use peritus_journal::StoreId;
use sha2::{Digest, Sha256};

/// Stable endpoint and lock identity derived from the exact C0 store identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DaemonIdentity {
    store_id: StoreId,
    endpoint_name: String,
}

impl DaemonIdentity {
    /// Derives a stable, non-secret endpoint name from one store identity.
    ///
    /// The name is `peritus-` followed by 16 hex characters. It is kept short because Unix socket
    /// paths are limited to 103 bytes on macOS (`sun_path`), and the macOS state root under
    /// `~/Library/Application Support/Peritus/State/daemon` already consumes most of that.
    #[must_use]
    pub fn new(store_id: StoreId) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/daemon-endpoint/v2\0");
        hasher.update(store_id.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        let mut endpoint_name = String::with_capacity(8 + 16);
        endpoint_name.push_str("peritus-");
        for byte in &digest[..8] {
            use core::fmt::Write as _;
            write!(&mut endpoint_name, "{byte:02x}").expect("writing into String cannot fail");
        }
        Self { store_id, endpoint_name }
    }

    /// Returns the exact journal store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Borrows the stable filesystem/pipe-safe endpoint name.
    #[must_use]
    pub fn endpoint_name(&self) -> &str {
        &self.endpoint_name
    }
}
