//! Provider discovery and account authentication without credential custody.

mod account;
mod direct;
mod error;
mod effects;
mod install;
mod models;
mod status;

pub use account::{AccountLogin, AccountProvider, ProviderCatalog};
pub use direct::{DirectCredential, DirectProviderDraft, remove_direct_credential};
pub use error::OnboardingError;
pub use effects::ProviderEffectStore;
pub use install::install_account_provider;
#[cfg(windows)]
pub use install::{account_installer_owner_argument, run_account_installer_owner};
pub use status::{ProviderObservation, ProviderStatus};
