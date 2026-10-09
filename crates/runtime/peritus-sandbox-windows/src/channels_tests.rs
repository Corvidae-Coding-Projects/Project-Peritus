use super::*;
use peritus_secrets::{SecretDeliveryContext, SecretLease, SecretLeaseId, SecretMaterial};
use peritus_types::{EnvironmentId, ProcessId, ResourceId, Sha256Digest};

#[test]
fn failed_cleanup_retains_exact_secret_owner_and_original_typed_cause_for_retry() {
    let root = tempfile::tempdir().unwrap();
    let owner = ProcessId::new([1; 16]).unwrap();
    let environment = EnvironmentId::new([2; 16]).unwrap();
    let digest = Sha256Digest::new([3; 32]);
    let reference =
        peritus_sandbox::SecretReference::new(ResourceId::new([4; 16]).unwrap(), digest);
    let lease = SecretLease::new(
        SecretLeaseId::new([5; 16]),
        owner,
        environment,
        digest,
        digest,
        reference,
        SecretDelivery::File(peritus_sandbox::SandboxPath::new("/credential").unwrap()),
        1,
        100,
    )
    .unwrap();
    let mut session = SecretDeliverySession::new();
    session
        .deliver(
            lease,
            SecretMaterial::new(b"canary".to_vec()).unwrap(),
            SecretDeliveryContext::new(owner, environment, digest, digest, 1),
            root.path(),
        )
        .unwrap();
    let target = session.artifacts()[0].file_paths().unwrap().0.to_owned();
    let saved = root.path().join("saved");
    std::fs::rename(&target, &saved).unwrap();
    std::fs::create_dir(&target).unwrap();
    let mut channels = PreparedChannels {
        network: NetworkIsolation::DenyAll,
        secrets: Vec::new(),
        handles: Vec::new(),
        proxy_owner: None,
        filter_owner: NetworkFilterOwner::inactive(),
        secret_owner: Some(session),
    };
    let original = channel_error(WindowsErrorKind::Handle, "staging failed")
        .with_source(WindowsErrorSource::Io(std::io::ErrorKind::PermissionDenied));
    let error = channels.cleanup(original);
    assert!(error.preparation_cleanup().secret_release());
    assert_eq!(error.kind(), WindowsErrorKind::Handle);
    assert_eq!(error.cause(), Some(WindowsErrorSource::Io(std::io::ErrorKind::PermissionDenied)));
    assert!(!error.retry_cleanup());
    assert!(!format!("{error:?}").contains("canary"));
    std::fs::remove_dir(&target).unwrap();
    std::fs::rename(&saved, &target).unwrap();
    assert!(error.retry_cleanup());
    assert!(!target.exists());
    assert!(error.retry_cleanup());
}

#[path = "../tests/support/mod.rs"]
mod support;

#[test]
fn deny_all_ignores_available_proxy_without_consuming_its_owner() {
    use peritus_network::{
        DnsMode, ManagedProxyPreparation, NetworkBounds, ProxyMode, RedirectMode, RoutingToken,
        RuntimeNetworkOptions, SystemResolver,
    };
    let root = tempfile::tempdir().unwrap();
    let options = RuntimeNetworkOptions::new(
        DnsMode::ProxySystem,
        RedirectMode::Deny,
        ProxyMode::HttpConnect,
        NetworkBounds::new(2, 1, 8192, 16384, 1000, 5000, 16, 4096).unwrap(),
        Vec::new(),
    );
    let preparation = ManagedProxyPreparation::new(
        options,
        RoutingToken::new([0x31; 32]),
        std::sync::Arc::new(SystemResolver),
        None,
    );
    let config = WindowsBackendConfig::new(
        root.path().join("helper"),
        crate::WindowsPath::new("C:/workspace").unwrap(),
        Vec::new(),
        root.path().join("acl"),
        crate::TokenProfile::AppContainer(
            crate::AppContainerProfile::new("Peritus.Test", "S-1-15-2-123").unwrap(),
        ),
        Some(Sha256Digest::new([1; 32])),
        Some(preparation),
        None,
    )
    .unwrap();
    let plan = support::checked_plan(Vec::new());
    assert_eq!(preflight_network(&config, &plan, false, false).unwrap(), NetworkIsolation::DenyAll);
    assert!(config.has_proxy_preparation());
    assert!(!root.path().join("acl").exists());
}
