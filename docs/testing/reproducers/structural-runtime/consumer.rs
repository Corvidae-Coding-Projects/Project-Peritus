//! Native consumers of the real publication, candidate, and plugin contracts; no test doubles.
use peritus_plugin_host::*;
use peritus_plugin_sdk::{PluginId, PluginQuotas, PluginVersion};
use peritus_product_state::publication::PublishedRecords;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
struct Deny;
impl AuthorityMediator for Deny {
    fn authorize<'a>(
        &'a self,
        _: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        Box::pin(async {
            Ok(AuthorityDecision::Denied {
                code: "fixture".into(),
                detail: "lifecycle only".into(),
            })
        })
    }
}
fn publication(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let registry =
        Arc::new(PublishedRecords::new(BTreeMap::from([(0u32, vec![0u8]), (1, vec![0u8])])));
    let (entered, wait) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let r = Arc::clone(&registry);
    let file = root.join("durable-publication.bin");
    let writer = std::thread::spawn(move || {
        let mut pending = r.write().unwrap();
        let value = pending.get_mut(&0).unwrap();
        value.push(1);
        entered.send(()).unwrap();
        resume.recv().unwrap();
        let f = std::fs::File::create(file).unwrap();
        use std::io::Write;
        (&f).write_all(value).unwrap();
        f.sync_all().unwrap();
    });
    wait.recv_timeout(Duration::from_secs(2))?;
    let now = Instant::now();
    assert_eq!(&**registry.read()?.get(&0).ok_or("missing key")?, &[0]);
    {
        let mut pending = registry.write()?;
        pending.get_mut(&1).ok_or("missing other key")?.push(9);
    }
    let independent_ms = now.elapsed().as_secs_f64() * 1000.;
    let same = Arc::clone(&registry);
    let (done, received) = mpsc::channel();
    let queued = std::thread::spawn(move || {
        let mut p = same.write().unwrap();
        p.get_mut(&0).unwrap().push(2);
        done.send(()).unwrap();
    });
    assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
    release.send(())?;
    writer.join().unwrap();
    received.recv_timeout(Duration::from_secs(2))?;
    queued.join().unwrap();
    assert_eq!(&**registry.read()?.get(&0).ok_or("key disappeared")?, &[0, 1, 2]);
    assert_eq!(std::fs::read(root.join("durable-publication.bin"))?, [0, 1]);
    let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&loads);
    registry.insert_deferred(2, vec![], move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(vec![42])
    })?;
    assert_eq!(registry.read()?.hints().count(), 3);
    assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 0);
    let threads = (0..8)
        .map(|_| {
            let r = Arc::clone(&registry);
            std::thread::spawn(move || {
                assert_eq!(&**r.read().unwrap().get(&2).unwrap(), &[42]);
            })
        })
        .collect::<Vec<_>>();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 1);
    registry.insert_deferred(3, vec![], {
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        move || {
            if attempt.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                None
            } else {
                Some(vec![7])
            }
        }
    })?;
    assert!(registry.read()?.get(&3).is_none());
    assert_eq!(&**registry.read()?.get(&3).ok_or("transient failure stuck")?, &[7]);
    let readers = registry.read()?;
    let threads = (10..14)
        .map(|key| {
            let r = Arc::clone(&registry);
            std::thread::spawn(move || {
                for i in 0..64u8 {
                    let mut p = r.write().unwrap();
                    if i == 0 {
                        p.insert(key, vec![]);
                    }
                    p.get_mut(&key).unwrap().push(i);
                }
            })
        })
        .collect::<Vec<_>>();
    for t in threads {
        t.join().unwrap();
    }
    assert!(readers.get(&10).is_none());
    for key in 10..14 {
        assert_eq!(registry.read()?.get(&key).unwrap().len(), 64);
    }
    println!(
        "{}",
        serde_json::json!({"case":"publication","independent_ms":independent_ms,"same_key_serialized":true,"durable_before_publish":true,"lazy_once":true,"retry_after_loader_failure":true,"concurrent_updates":256})
    );
    Ok(())
}
async fn plugins(root: &Path, executable: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for mode in ["normal", "crash", "flood", "malformed", "timeout"] {
        let base = root.join(mode);
        let dir = base.join("plugin");
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(executable, dir.join("plugin"))?;
        let text = format!(
            r#"manifest_version = 1
id = "real.plugin.{mode}"
version = {{ major = 1, minor = 0, patch = 0 }}
kind = "process"
capabilities = []
[protocol]
minimum = 1
maximum = 1
[entrypoint]
artifact = "plugin"
arguments = ["{mode}"]
[quotas]
concurrent_requests = 1
frame_bytes = 65536
output_bytes = 65536
invocation_millis = 1000
lifecycle_requests = 10
protocol_violations = 1
"#
        );
        std::fs::write(dir.join("peritus-plugin.toml"), text)?;
        let catalog = discover(&[base], DiscoveryLimits::PRODUCTION)?;
        let found = catalog.iter().next().ok_or("undiscovered plugin")?;
        let trust = DigestTrustStore::new().with_anchor(
            found.manifest().id().clone(),
            found.manifest().version(),
            found.manifest_digest(),
            found.artifact_sha256(),
            "owned fixture",
        );
        let quotas = PluginQuotas {
            concurrent_requests: 1,
            frame_bytes: 65536,
            output_bytes: 65536,
            invocation_millis: 1000,
            lifecycle_requests: 10,
            protocol_violations: 1,
        };
        let host = PluginHost::new(
            HostConfig {
                wasm_runtime: PathBuf::from("unused"),
                quota_ceiling: quotas,
                startup_timeout: Duration::from_millis(800),
                shutdown_timeout: Duration::from_secs(2),
            },
            catalog,
            Arc::new(Deny),
            Arc::new(trust),
        );
        let id = PluginId::new(format!("real.plugin.{mode}"))?;
        let now = Instant::now();
        let result = host.start(&id, PluginVersion::new(1, 0, 0)).await;
        let detail = if mode == "normal" {
            result?;
            host.stop(&id).await?;
            String::new()
        } else {
            let error = result.expect_err("failing native plugin was accepted");
            if mode == "malformed" {
                assert_eq!(error.class(), HostFailureClass::Protocol);
            }
            if mode == "timeout" {
                assert_eq!(error.class(), HostFailureClass::Timeout);
            }
            let text = error.to_string();
            assert!(text.contains("stderr tail"), "{mode}: {text}");
            assert!(
                text.contains(if mode == "crash" || mode == "flood" {
                    "REAL-PLUGIN-FAILURE"
                } else if mode == "malformed" {
                    "REAL-PLUGIN-MALFORMED"
                } else {
                    "REAL-PLUGIN-TIMEOUT"
                }),
                "{text}"
            );
            assert!(text.len() < 18000);
            if mode == "crash" || mode == "flood" {
                assert!(text.contains("23"));
            }
            text
        };
        let pid = std::fs::read_to_string(dir.join("child.pid"))?;
        assert!(!PathBuf::from(format!("/proc/{}", pid.trim())).exists(), "child survived {mode}");
        std::fs::write(root.join(format!("{mode}-diagnostic.txt")), &detail)?;
        println!(
            "{}",
            serde_json::json!({"case":mode,"seconds":now.elapsed().as_secs_f64(),"diagnostic_bytes":detail.len(),"child_reaped":true})
        );
    }
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args_os().collect::<Vec<_>>();
    let root = Path::new(&a[2]);
    std::fs::create_dir_all(root)?;
    match a[1].to_str().ok_or("mode")? {
        "publication" => publication(root)?,
        "plugins" => plugins(root, Path::new(&a[3])).await?,
        "identity" => println!("{:?}", peritus_workspace::task_candidate::identity(root)?),
        _ => return Err("mode".into()),
    };
    Ok(())
}
