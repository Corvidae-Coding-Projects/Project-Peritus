//! Exact secret leases, zeroizing material, scoped delivery, and redaction support.

mod delivery;
mod error;
mod fingerprint;
mod lease;
mod material;
mod preparation;
mod recovery;
mod refinement;
mod store;
mod verified;

pub use delivery::{
    DeliveryArtifact, DeliveryReceipt, SecretDeliveryContext, SecretDeliverySession,
};
pub use error::{RecoveryClass, SecretError, SecretErrorKind, SecretOperation};
pub use fingerprint::{RedactionFingerprint, RedactionSet};
pub use lease::{SecretLease, SecretLeaseId, SecretLeaseState};
pub use material::SecretMaterial;
pub use preparation::SecretPreparation;
pub use recovery::{SecretRecoveryRecord, SecretRecoveryState};
pub use refinement::secret_delivery_exact;
pub use store::{CredentialStore, PlatformCredentialStore, StoreProbe};
#[cfg(any(test, feature = "test-memory-store"))]
pub use store::{MemoryCredentialStore, MemoryStoreOutcome};
