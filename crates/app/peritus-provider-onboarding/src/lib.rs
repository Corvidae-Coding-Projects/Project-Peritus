//! Provider discovery and account authentication without credential custody.

mod account;
mod direct;
mod error;
mod install;
mod models;
mod status;

pub use account::{AccountLogin, AccountProvider, ProviderCatalog};
pub use direct::{DirectCredential, DirectProviderDraft, remove_direct_credential};
pub use error::OnboardingError;
pub use install::install_account_provider;
pub use status::{ProviderObservation, ProviderStatus};
