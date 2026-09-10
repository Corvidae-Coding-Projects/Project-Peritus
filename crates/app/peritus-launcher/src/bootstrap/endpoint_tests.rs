use super::{AppLayout, ProductBootstrap};
use peritus_daemon::{DaemonRuntime, LocalEndpointAddress};

#[tokio::test]
async fn long_installation_bootstrap_and_daemon_agree_on_the_connectable_endpoint() {
    let temporary = tempfile::tempdir().expect("temporary installation");
    let root = temporary.path().join("long-home-".repeat(18)).join("nested-home-".repeat(12));
    let layout = AppLayout::for_test(&root).prepare().expect("long installation layout");
    let product = ProductBootstrap::new(layout.clone()).prepare().expect("bootstrap");
    let config = product.daemon_config();
    assert!(config.paths().state_root().as_os_str().len() > 252);
    let runtime = DaemonRuntime::start(config.clone()).await.expect("real daemon startup");
    let LocalEndpointAddress::Unix(actual) = runtime.endpoint_address();
    assert_eq!(&product.endpoint_path(), actual);
    let _client = tokio::net::UnixStream::connect(product.endpoint_path())
        .await
        .expect("launcher address connects");
    let repeated = ProductBootstrap::new(layout).prepare().expect("repeat bootstrap");
    assert_eq!(repeated.endpoint_path(), product.endpoint_path());
    runtime.shutdown().await.expect("daemon shutdown");
    assert!(!product.endpoint_path().exists(), "daemon owns endpoint cleanup");
}
