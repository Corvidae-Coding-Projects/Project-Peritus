use std::{fs, os::unix::fs::PermissionsExt as _, time::Duration};

use peritus_journal::StoreId;

use super::{ACCEPT_RETRY_DELAY, accept_after};
use crate::{DaemonIdentity, LocalEndpoint, LocalEndpointAddress};

#[tokio::test]
async fn delayed_accept_remains_cancellable_without_consuming_the_queued_client() {
    let root = tempfile::tempdir().expect("state root");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).expect("protected root");
    let identity = DaemonIdentity::new(StoreId::new([1; 16]).expect("store"));
    let endpoint = LocalEndpoint::bind(root.path(), &identity).await.expect("endpoint");
    let LocalEndpointAddress::Unix(path) = endpoint.address();
    let _client = tokio::net::UnixStream::connect(path).await.expect("queued connection");
    let retry_at = Some(tokio::time::Instant::now() + ACCEPT_RETRY_DELAY);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), accept_after(&endpoint, retry_at))
            .await
            .is_err()
    );
    let accepted = tokio::time::timeout(Duration::from_secs(2), accept_after(&endpoint, retry_at))
        .await
        .expect("bounded retry")
        .expect("same queued client is authenticated");
    assert_eq!(accepted.peer(), endpoint.owner_peer());
}
