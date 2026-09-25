//! C3-backed lazy credential resolution for direct C5 adapters.

use peritus_provider_core::{Credential, CredentialReference, CredentialSource, ProviderCoreError};
use peritus_secrets::{
    CredentialStore, PlatformCredentialStore, SecretError, SecretErrorKind,
    parse_credential_reference,
};

/// Platform credential-store adapter understood by the daemon's opaque C5 references.
#[derive(Debug)]
pub struct PlatformCredentialSource {
    store: PlatformCredentialStore,
}

impl PlatformCredentialSource {
    /// Opens the same provider namespace used by setup without reading any secret material.
    #[must_use]
    pub fn providers() -> Self {
        Self { store: PlatformCredentialStore::providers() }
    }

    /// Reports whether the current platform credential adapter is available.
    #[must_use]
    pub fn available(&self) -> bool {
        self.store.probe().available()
    }
}

impl CredentialSource for PlatformCredentialSource {
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        let reference = parse_credential_reference(reference.as_str())
            .map_err(|_| credential_error("credential reference is malformed"))?;
        let material = self.store.lookup(reference).map_err(lookup_error)?;
        material.expose(|bytes| Credential::new(bytes.to_vec()))
    }
}

const fn lookup_error(error: SecretError) -> ProviderCoreError {
    credential_error(match error.kind() {
        SecretErrorKind::Missing => {
            "saved provider credential is missing from this user's credential store; run peritus providers repair under the same OS account as peritusd"
        }
        SecretErrorKind::Locked | SecretErrorKind::Denied => {
            "provider credential store is locked or access was denied; unlock the store and run peritusd under the account used for setup"
        }
        SecretErrorKind::StaleVersion => {
            "saved provider credential differs from its configured version; run peritus providers repair"
        }
        SecretErrorKind::Unavailable => {
            "platform credential store is unavailable to peritusd; check the OS account and credential service"
        }
        SecretErrorKind::Corrupt | SecretErrorKind::InvalidInput => {
            "saved provider credential is malformed; run peritus providers repair"
        }
        _ => {
            "platform credential lookup failed during an OS operation; check the credential service and retry"
        }
    })
}

const fn credential_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::credential(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_secrets::{RecoveryClass, SecretOperation};

    #[test]
    fn lookup_failures_explain_recovery_without_echoing_store_contents() {
        for (kind, expected) in [
            (SecretErrorKind::Missing, "same OS account"),
            (SecretErrorKind::StaleVersion, "configured version"),
            (SecretErrorKind::Locked, "unlock the store"),
            (SecretErrorKind::Unavailable, "credential service"),
            (SecretErrorKind::Corrupt, "providers repair"),
        ] {
            let error = lookup_error(SecretError::new(
                kind,
                SecretOperation::Lookup,
                RecoveryClass::Retry,
                "PRIVATE_STORE_CANARY",
            ));
            let display = error.to_string();
            assert!(display.contains(expected), "{display}");
            assert!(!display.contains("PRIVATE_STORE_CANARY"));
        }
    }

    #[cfg(windows)]
    struct Cleanup(peritus_types::ResourceId);

    #[cfg(windows)]
    impl Drop for Cleanup {
        fn drop(&mut self) {
            PlatformCredentialStore::providers()
                .remove(self.0)
                .expect("remove disposable credential");
        }
    }

    // Setup and peritusd are separate processes. Exercise the real Windows vault and the actual
    // daemon adapter, using only an isolated disposable entry; never read configured credentials.
    #[cfg(windows)]
    #[test]
    fn windows_provider_credential_survives_process_restart() {
        const CHILD_REF: &str = "PERITUS_TEST_PROVIDER_VAULT_REFERENCE";
        const SECRET: &[u8] = b"peritus-disposable-vault-fixture";
        if let Ok(reference) = std::env::var(CHILD_REF) {
            let source = PlatformCredentialSource::providers();
            assert!(source.available());
            let reference = CredentialReference::new(reference).unwrap();
            assert_eq!(
                source.resolve(&reference).expect("daemon lookup after restart").len(),
                SECRET.len()
            );
            return;
        }
        let temporary = tempfile::tempdir().unwrap();
        let digest = peritus_codec::sha256(temporary.path().to_string_lossy().as_bytes());
        let resource =
            peritus_types::ResourceId::new(digest.as_bytes()[..16].try_into().unwrap()).unwrap();
        let store = PlatformCredentialStore::providers();
        let reference = store
            .store(resource, &peritus_secrets::SecretMaterial::new(SECRET.to_vec()).unwrap())
            .expect("setup stores credential");
        let _cleanup = Cleanup(resource);
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "component::credentials::tests::windows_provider_credential_survives_process_restart", "--nocapture"])
            .env(CHILD_REF, peritus_secrets::format_credential_reference(reference))
            .output().expect("spawn daemon credential lookup");
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
    }
}
