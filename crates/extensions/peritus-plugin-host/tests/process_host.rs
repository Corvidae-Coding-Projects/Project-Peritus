//! Out-of-process G3 host lifecycle and mediation acceptance tests.

#![cfg(unix)]

use std::{
    fs,
    future::Future,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use peritus_plugin_host::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DigestTrustStore, DiscoveryLimits,
    HostCancellation, HostConfig, HostError, HostFailureClass, HostFuture, InvocationGrant,
    InvocationSubject, PluginHost, PluginInvocationResult, PluginLifecycle, discover,
};
use peritus_plugin_sdk::{
    JsonBounds, JsonPayload, PluginId, PluginQuotas, PluginVersion, RequestId,
};
use serde_json::Value;
use tempfile::TempDir;

#[path = "process_host/conformance.rs"]
mod conformance;

const MANIFEST: &str = r#"
manifest_version = 1
id = "corvidae.fixture"
kind = "process"

[version]
major = 1
minor = 0
patch = 0

[protocol]
minimum = 1
maximum = 1

[entrypoint]
artifact = "fixture.py"
arguments = []

[[capabilities]]
name = "fs.read"
operation = "inspection"
required = true

[quotas]
concurrent_requests = 2
frame_bytes = 65536
output_bytes = 4096
invocation_millis = 5000
lifecycle_requests = 16
protocol_violations = 2
"#;

// Hosted macOS runners can take several seconds to schedule a freshly spawned
// interpreter. This budget covers process initialization only; invocation and
// cancellation deadlines remain governed by their own stricter quotas.
const FIXTURE_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

const FIXTURE: &str = r#"#!/usr/bin/env python3
import json
import pathlib
import struct
import sys
import time

def receive():
    header = sys.stdin.buffer.read(4)
    if not header:
        return None
    size = struct.unpack(">I", header)[0]
    body = sys.stdin.buffer.read(size)
    if len(body) != size:
        raise SystemExit(2)
    return json.loads(body)

def send(request, response):
    message = {
        "protocol_version": request["protocol_version"],
        "request_id": request["request_id"],
        "response": response,
    }
    body = json.dumps(message, separators=(",", ":")).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(body)))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()

while True:
    request = receive()
    if request is None:
        break
    method = request["request"]["method"]
    if method == "initialize":
        send(request, {"kind": "status", "body": {"status": "ready"}})
    elif method == "shutdown":
        send(request, {"kind": "status", "body": {"status": "stopped"}})
        break
    elif method == "invoke":
        pathlib.Path("invocations.log").open("a", encoding="utf-8").write("invoke\n")
        value = request["request"]["params"]["input"]
        mode = value.get("mode") if isinstance(value, dict) else None
        if mode == "crash":
            raise SystemExit(9)
        if mode == "sleep":
            time.sleep(30)
        output = {"value": "x" * 2048} if mode == "oversize" else {"echo": value}
        send(request, {"kind": "success", "body": {"output": output, "rendering": "ok"}})
    else:
        send(request, {"kind": "failure", "body": {
            "class": "unsupported", "code": "unsupported", "detail": method,
            "retryable_with_new_action": False
        }})
"#;

struct Allow;

impl AuthorityMediator for Allow {
    fn authorize<'a>(
        &'a self,
        request: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        Box::pin(async move {
            Ok(AuthorityDecision::Authorized(InvocationGrant::observed(
                vec![request.capability().name().to_owned()],
                10_000,
            )))
        })
    }
}

struct Deny;

impl AuthorityMediator for Deny {
    fn authorize<'a>(
        &'a self,
        _request: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        Box::pin(async {
            Ok(AuthorityDecision::Denied {
                code: "policy_denied".to_owned(),
                detail: "fixture authority denied".to_owned(),
            })
        })
    }
}

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    catalog: peritus_plugin_host::PluginCatalog,
    id: PluginId,
    version: PluginVersion,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary plugin root");
        let root = temporary.path().join("plugins");
        let plugin = root.join("fixture");
        fs::create_dir_all(&plugin).expect("plugin directory");
        fs::write(plugin.join("peritus-plugin.toml"), MANIFEST).expect("manifest");
        let artifact = plugin.join("fixture.py");
        fs::write(&artifact, FIXTURE).expect("fixture executable");
        let mut permissions = fs::metadata(&artifact).expect("artifact metadata").permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&artifact, permissions).expect("executable permissions");
        let root = fs::canonicalize(root).expect("canonical root");
        let catalog =
            discover(std::slice::from_ref(&root), DiscoveryLimits::PRODUCTION).expect("discovery");
        Self {
            _temporary: temporary,
            root,
            catalog,
            id: PluginId::new("corvidae.fixture").expect("plugin id"),
            version: PluginVersion::new(1, 0, 0),
        }
    }

    fn trust(&self) -> DigestTrustStore {
        let plugin = self.catalog.get(&self.id, self.version).expect("discovered plugin");
        DigestTrustStore::new().with_anchor(
            self.id.clone(),
            self.version,
            plugin.manifest_digest(),
            plugin.artifact_sha256(),
            "fixture-test-anchor",
        )
    }

    fn config(output_bytes: u64) -> HostConfig {
        HostConfig {
            wasm_runtime: PathBuf::from("wasmtime"),
            quota_ceiling: PluginQuotas {
                concurrent_requests: 2,
                frame_bytes: 65_536,
                output_bytes,
                invocation_millis: 5_000,
                lifecycle_requests: 16,
                protocol_violations: 2,
            },
            startup_timeout: FIXTURE_STARTUP_TIMEOUT,
            shutdown_timeout: Duration::from_secs(2),
        }
    }

    fn log(&self) -> PathBuf {
        self.root.join("fixture").join("invocations.log")
    }
}

fn subject() -> InvocationSubject {
    InvocationSubject::new("session-1", "actor-1", 7)
}

fn payload(value: Value) -> JsonPayload {
    JsonPayload::new(value, JsonBounds::PRODUCTION).expect("payload")
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("test JSON")
}

fn run_async<F>(future: F)
where
    F: Future<Output = ()>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future);
}

fn host(fixture: &Fixture, authority: Arc<dyn AuthorityMediator>, output_bytes: u64) -> PluginHost {
    PluginHost::new(
        Fixture::config(output_bytes),
        fixture.catalog.clone(),
        authority,
        Arc::new(fixture.trust()),
    )
}

#[test]
fn discovery_trust_lifecycle_and_invocation_are_real_process_boundaries() {
    run_async(async {
        let fixture = Fixture::new();
        assert_eq!(fixture.catalog.len(), 1);
        let discovered = fixture.catalog.get(&fixture.id, fixture.version).expect("plugin");
        assert!(discovered.artifact_path().starts_with(discovered.root()));

        let untrusted = PluginHost::new(
            Fixture::config(4_096),
            fixture.catalog.clone(),
            Arc::new(Allow),
            Arc::new(DigestTrustStore::new()),
        );
        let error = untrusted
            .start(&fixture.id, fixture.version)
            .await
            .expect_err("untrusted plugin rejected");
        assert_eq!(error.class(), HostFailureClass::Trust);
        assert!(untrusted.snapshots().await.is_empty());

        let trusted = host(&fixture, Arc::new(Allow), 4_096);
        trusted.start(&fixture.id, fixture.version).await.expect("start trusted plugin");
        let snapshots = trusted.snapshots().await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].lifecycle, PluginLifecycle::Ready);
        assert_eq!(snapshots[0].trust_anchor, "fixture-test-anchor");

        let result = trusted
            .invoke(
                &fixture.id,
                RequestId::new("invoke-1").expect("request id"),
                "fs.read",
                payload(json(r#"{"path":"README.md"}"#)),
                &subject(),
                &HostCancellation::new(),
            )
            .await
            .expect("invoke plugin");
        let PluginInvocationResult::Succeeded { output, rendering } = result else {
            panic!("expected plugin success");
        };
        assert_eq!(output.value()["echo"]["path"], "README.md");
        assert_eq!(rendering.as_deref(), Some("ok"));
        assert_eq!(fs::read_to_string(fixture.log()).expect("invocation log"), "invoke\n");
        trusted.stop(&fixture.id).await.expect("stop plugin");
        assert!(trusted.snapshots().await.is_empty());
    });
}

#[test]
fn authority_denial_occurs_before_plugin_effect() {
    run_async(async {
        let fixture = Fixture::new();
        let denied = host(&fixture, Arc::new(Deny), 4_096);
        denied.start(&fixture.id, fixture.version).await.expect("start plugin");
        let error = denied
            .invoke(
                &fixture.id,
                RequestId::new("denied-1").expect("request id"),
                "fs.read",
                payload(json(r#"{"path":"README.md"}"#)),
                &subject(),
                &HostCancellation::new(),
            )
            .await
            .expect_err("authority denial");
        assert_eq!(error.class(), HostFailureClass::Authorization);
        assert!(!fixture.log().exists());
        denied.stop(&fixture.id).await.expect("stop plugin");
    });
}

#[test]
fn host_output_ceiling_and_duplicate_start_are_enforced() {
    run_async(async {
        let fixture = Fixture::new();
        let hosted = Arc::new(host(&fixture, Arc::new(Allow), 128));
        let left = {
            let hosted = Arc::clone(&hosted);
            let id = fixture.id.clone();
            tokio::spawn(async move { hosted.start(&id, PluginVersion::new(1, 0, 0)).await })
        };
        let right = {
            let hosted = Arc::clone(&hosted);
            let id = fixture.id.clone();
            tokio::spawn(async move { hosted.start(&id, PluginVersion::new(1, 0, 0)).await })
        };
        let outcomes = [left.await.expect("left join"), right.await.expect("right join")];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);

        let error = hosted
            .invoke(
                &fixture.id,
                RequestId::new("oversize-1").expect("request id"),
                "fs.read",
                payload(json(r#"{"mode":"oversize"}"#)),
                &subject(),
                &HostCancellation::new(),
            )
            .await
            .expect_err("host ceiling rejects output");
        assert_eq!(error.class(), HostFailureClass::Quota);
        assert_eq!(hosted.snapshots().await[0].lifecycle, PluginLifecycle::Failed);
    });
}

#[test]
fn cancellation_terminates_the_owned_plugin_process() {
    run_async(async {
        let fixture = Fixture::new();
        let hosted = Arc::new(host(&fixture, Arc::new(Allow), 4_096));
        hosted.start(&fixture.id, fixture.version).await.expect("start plugin");
        let cancellation = HostCancellation::new();
        let invocation = {
            let hosted = Arc::clone(&hosted);
            let id = fixture.id.clone();
            let cancellation = cancellation.clone();
            tokio::spawn(async move {
                hosted
                    .invoke(
                        &id,
                        RequestId::new("slow-1").expect("request id"),
                        "fs.read",
                        payload(json(r#"{"mode":"sleep"}"#)),
                        &subject(),
                        &cancellation,
                    )
                    .await
            })
        };
        wait_for_file(&fixture.log()).await;
        assert!(cancellation.cancel());
        let error = tokio::time::timeout(Duration::from_secs(2), invocation)
            .await
            .expect("cancellation deadline")
            .expect("invocation join")
            .expect_err("cancelled invocation");
        assert_eq!(error.class(), HostFailureClass::Cancelled);
        assert_eq!(hosted.snapshots().await[0].lifecycle, PluginLifecycle::Failed);
    });
}

async fn wait_for_file(path: &Path) {
    for _ in 0..100 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("plugin did not begin invocation");
}
