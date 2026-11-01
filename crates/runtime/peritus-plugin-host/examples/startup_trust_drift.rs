//! Sequential artifact-byte drift must not inherit an old exact-digest trust anchor.
use peritus_plugin_host::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DigestTrustStore, DiscoveryLimits,
    HostConfig, HostError, HostFailureClass, HostFuture, PluginHost, RecoveryDisposition, discover,
};
use peritus_plugin_sdk::PluginVersion;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
type Outcome<T> = Result<T, Box<dyn Error>>;
struct Deny(AtomicUsize);
impl AuthorityMediator for Deny {
    fn authorize<'a>(
        &'a self,
        _: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(AuthorityDecision::Denied {
                code: "no-effects".into(),
                detail: "This fixture authorizes no invocation".into(),
            })
        })
    }
}
const SCRIPT: &str = r"#!/usr/bin/python3
import json,os,struct,sys
with open('started.txt','w') as marker:marker.write('RELEASE-A');marker.flush()
def readn(n):
 result=b''
 while len(result)<n:
  part=sys.stdin.buffer.read(n-len(result))
  if not part:return None
  result+=part
 return result
while True:
 header=readn(4)
 if header is None:break
 raw=readn(struct.unpack('>I',header)[0])
 if raw is None:break
 request=json.loads(raw)
 status='stopped' if request['request']['method']=='shutdown' else 'ready'
 response={'protocol_version':1,'request_id':request['request_id'],'response':{'kind':'status','body':{'status':status}}}
 payload=json.dumps(response,separators=(',',':')).encode()
 sys.stdout.buffer.write(struct.pack('>I',len(payload))+payload);sys.stdout.buffer.flush()
 if status=='stopped':break
";
const MANIFEST: &str = r#"manifest_version=1
id="fixture.trust"
version={major=1,minor=0,patch=0}
kind="process"
protocol={minimum=1,maximum=1}
entrypoint={artifact="plugin",arguments=[]}
capabilities=[]
quotas={concurrent_requests=1,frame_bytes=4096,output_bytes=128,invocation_millis=300,lifecycle_requests=2,protocol_violations=1}
"#;
async fn run(root: &Path) -> Outcome<()> {
    fs::create_dir(root)?;
    let package = root.join("fixture");
    fs::create_dir(&package)?;
    let executable = package.join("plugin");
    fs::write(&executable, SCRIPT)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    }
    fs::write(package.join("peritus-plugin.toml"), MANIFEST)?;
    let catalog = discover(&[root.to_path_buf()], DiscoveryLimits::PRODUCTION)?;
    let plugin = catalog.iter().next().ok_or("fixture not discovered")?;
    let id = plugin.manifest().id().clone();
    let initial_digest = plugin.artifact_sha256();
    let quotas = plugin.manifest().quotas();
    assert_eq!(initial_digest, <[u8; 32]>::from(Sha256::digest(fs::read(&executable)?)));
    let trust = DigestTrustStore::new().with_anchor(
        id.clone(),
        PluginVersion::new(1, 0, 0),
        plugin.manifest_digest(),
        initial_digest,
        "exact release A",
    );
    let authority = Arc::new(Deny(AtomicUsize::new(0)));
    let host = PluginHost::new(
        HostConfig {
            wasm_runtime: "/nonexistent-fixture-runtime".into(),
            quota_ceiling: quotas,
            startup_timeout: Duration::from_secs(3),
            shutdown_timeout: Duration::from_millis(300),
        },
        catalog,
        authority.clone(),
        Arc::new(trust),
    );
    host.start(&id, PluginVersion::new(1, 0, 0)).await?;
    assert_eq!(fs::read_to_string(package.join("started.txt"))?, "RELEASE-A");
    host.stop(&id).await?;
    assert!(host.snapshots().await.is_empty());
    println!("PASS unchanged exact-trust startup and explicit shutdown");
    let changed = SCRIPT.replace("RELEASE-A", "RELEASE-B");
    assert_eq!(changed.len(), SCRIPT.len());
    let mut file = fs::OpenOptions::new().write(true).truncate(true).open(&executable)?;
    file.write_all(changed.as_bytes())?;
    file.sync_all()?;
    drop(file);
    let changed_digest = <[u8; 32]>::from(Sha256::digest(fs::read(&executable)?));
    assert_ne!(changed_digest, initial_digest);
    assert_eq!(fs::read_to_string(package.join("peritus-plugin.toml"))?, MANIFEST);
    println!("Original artifact SHA256: {initial_digest:02x?}");
    println!("Changed artifact SHA256: {changed_digest:02x?}");
    let result = host.start(&id, PluginVersion::new(1, 0, 0)).await;
    // Observe and reap only the host-owned fixture before returning any regression failure.
    let marker = fs::read_to_string(package.join("started.txt"))?;
    let _ = host.stop(&id).await;
    assert_eq!(
        authority.0.load(Ordering::SeqCst),
        0,
        "no invocation or effect authority requested"
    );
    assert!(host.snapshots().await.is_empty());
    match result {
  Err(error) if error.class()==HostFailureClass::Trust&&error.recovery()==RecoveryDisposition::EstablishTrust=>{
   assert_eq!(marker,"RELEASE-A","changed bytes ran before trust rejection");println!("PASS changed artifact rejected before launch under the stale anchor");Ok(())
  },
  Err(error)=>Err(format!("expected exact-trust refusal, received another failure: {error}; startup marker {marker}").into()),
  Ok(())=>Err(format!("changed artifact started under old exact-digest trust anchor; observed startup marker {marker}").into()),
 }
}
fn main() -> Outcome<()> {
    let root = std::env::args_os().nth(1).ok_or("pass a new owned disposable directory")?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(Path::new(&root)))
}
