//! Real public API consumer and executable process plugin. Run using module-foundation.md.
use peritus_modules::{Definition, Job};
use peritus_plugin_host::*;
use peritus_plugin_sdk::*;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn plugin(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    let mut initialized = false;
    loop {
        if initialized && mode == "block" {
            let child = std::process::Command::new("/bin/sleep").arg("600").spawn()?;
            fs::write("descendant.pid", child.id().to_string())?;
            std::thread::sleep(Duration::from_secs(600));
        }
        let mut header = [0; 4];
        if input.read_exact(&mut header).is_err() {
            return Ok(());
        }
        let mut frame = header.to_vec();
        frame.resize(4 + u32::from_be_bytes(header) as usize, 0);
        input.read_exact(&mut frame[4..])?;
        let request: PluginRequestEnvelope = decode_frame(&frame, 1048576)?;
        let response = match request.request {
            HostRequest::Initialize { .. } => {
                initialized = true;
                PluginResponse::Status { status: PluginStatus::Ready }
            }
            HostRequest::Shutdown => PluginResponse::Status {
                status: if mode == "bad-shutdown" {
                    PluginStatus::Ready
                } else {
                    PluginStatus::Stopped
                },
            },
            HostRequest::Invoke { input, .. } => {
                if mode == "wrong" {
                    output.write_all(b"\0\0\0\x01x")?;
                    output.flush()?;
                    continue;
                }
                if mode == "delay" {
                    std::thread::sleep(Duration::from_millis(200));
                }
                PluginResponse::Success {
                    output: input,
                    rendering: Some("actual plugin café 🦀".into()),
                }
            }
            _ => PluginResponse::Status { status: PluginStatus::Ready },
        };
        output.write_all(&encode_frame(
            &PluginResponseEnvelope {
                protocol_version: 1,
                request_id: request.request_id,
                response,
            },
            1048576,
        )?)?;
        output.flush()?;
    }
}
struct LocalAuthority(AtomicUsize);
impl AuthorityMediator for LocalAuthority {
    fn authorize<'a>(
        &'a self,
        r: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(AuthorityDecision::Authorized(InvocationGrant::observed(
                vec![r.capability().name().into()],
                999999,
            )))
        })
    }
}
fn payload(large: bool) -> JsonPayload {
    JsonPayload::new(
        serde_json::json!({"message":if large{"a".repeat(240000)}else{"real invocation".into()}}),
        JsonBounds::PRODUCTION,
    )
    .unwrap()
}
async fn host(
    base: &Path,
    mode: &str,
) -> Result<
    (Arc<PluginHost>, PluginId, PluginVersion, Arc<LocalAuthority>),
    Box<dyn std::error::Error>,
> {
    let directory = base.join(mode);
    let package = directory.join("example");
    fs::create_dir_all(&package)?;
    fs::copy(std::env::current_exe()?, package.join("plugin"))?;
    fs::write(
        package.join("peritus-plugin.toml"),
        format!(
            r#"manifest_version=1
id="example.plugin"
version={{major=1,minor=0,patch=0}}
kind="process"
protocol={{minimum=1,maximum=1}}
entrypoint={{artifact="plugin",arguments=["--plugin","{mode}"]}}
capabilities=[{{name="example.inspect",operation="inspection",required=true}}]
quotas={{concurrent_requests=2,frame_bytes=1048576,output_bytes=1048576,invocation_millis=800,lifecycle_requests=100,protocol_violations=1}}
"#
        ),
    )?;
    let catalog = discover(&[directory], DiscoveryLimits::PRODUCTION)?;
    let d = catalog.iter().next().unwrap();
    let id = d.manifest().id().clone();
    let version = d.manifest().version();
    let quotas = d.manifest().quotas();
    let trust = DigestTrustStore::new().with_anchor(
        id.clone(),
        version,
        d.manifest_digest(),
        d.artifact_sha256(),
        "explicit disposable fixture",
    );
    let authority = Arc::new(LocalAuthority(AtomicUsize::new(0)));
    let host = Arc::new(PluginHost::new(
        HostConfig {
            wasm_runtime: "/nonexistent".into(),
            quota_ceiling: quotas,
            startup_timeout: Duration::from_millis(500),
            shutdown_timeout: Duration::from_millis(150),
        },
        catalog,
        authority.clone(),
        Arc::new(trust),
    ));
    host.start(&id, version).await?;
    Ok((host, id, version, authority))
}
fn subject() -> InvocationSubject {
    InvocationSubject::new("real-session", "local-user", 1)
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--plugin") {
        return plugin(&args[2]);
    }
    let base = Path::new(&args[1]);
    fs::create_dir_all(base)?;
    let manifest = r#"schema=1
id="local.consumer"
version="1.0.0"
name="Consumer"
description="Public module API fixture"
[[roles]]
id="reader"
model="writer"
instructions="Read source"
tools=[]
[[stages]]
id="gather"
role="reader"
[[stages]]
id="assess"
role="reader"
inputs=["gather"]
"#;
    let package = base.join("package");
    fs::create_dir_all(&package)?;
    fs::write(package.join("module.toml"), manifest)?;
    let definition = Definition::from_directory(&package)?;
    for i in 0..24 {
        let task = format!("Task {i}: café 🦀, punctuation : / and newline\n");
        let text = format!("{i} {}", "é🦀•".repeat(i * 100 + 1));
        let mut job = Job::open(&base.join("jobs"), definition.clone(), i as u64, &task)?;
        assert!(Job::open(&base.join("jobs"), definition.clone(), i as u64, &task).is_err());
        assert!(job.complete("assess", "out of order").is_err());
        job.complete("gather", &text)?;
        assert!(job.read(&[], "gather", 0, 10).is_err());
        let mut offset = 0;
        let mut recovered = String::new();
        loop {
            let page = job.read(&["gather".into()], "gather", offset, 13)?;
            recovered.push_str(page["text"].as_str().unwrap());
            match page["next_offset"].as_u64() {
                Some(n) => offset = n,
                None => break,
            }
        }
        assert_eq!(recovered, text);
        assert!(job.result()?.is_none());
        let observed = Job::inspect(&base.join("jobs"), definition.clone(), i as u64, &task, 0)?;
        assert_eq!(observed["completed_stages"].as_array().unwrap().len(), 1);
        drop(job);
        let mut job = Job::open(&base.join("jobs"), definition.clone(), i as u64, &task)?;
        job.complete("assess", "accepted result")?;
        assert_eq!(job.result()?.as_deref(), Some("accepted result"));
        let artifact = job.root().join(format!("gather-{}.txt", job.state().completed[0].sha256));
        drop(job);
        fs::write(artifact, "corrupted")?;
        assert!(Job::open(&base.join("jobs"), definition.clone(), i as u64, &task).is_err());
    }
    println!(
        "24 varied durable module journeys passed (paging, dependency denial, lock, inspect, close/reopen, corruption)"
    );
    let h = HostError::new(
        HostFailureClass::Plugin,
        RecoveryDisposition::None,
        "unicode",
        format!("{}🦀", "x".repeat(1023)),
    );
    assert!(h.detail().len() <= 1024);
    let s =
        SdkError::new(SdkErrorKind::InvalidManifest, "unicode", format!("{}🦀", "x".repeat(511)));
    assert!(s.detail().len() <= 512);
    let (h, id, v, authority) = host(base, "delay").await?;
    let subject = subject();
    let c = HostCancellation::new();
    let c2 = c.clone();
    let h2 = h.clone();
    let id2 = id.clone();
    let active = tokio::spawn(async move {
        h2.invoke(
            &id2,
            RequestId::new("first").unwrap(),
            "example.inspect",
            payload(false),
            &subject,
            &c2,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    let cancellation = HostCancellation::new();
    let _ = cancellation.cancel();
    let unsent = h
        .invoke(
            &id,
            RequestId::new("queued").unwrap(),
            "example.inspect",
            payload(false),
            &crate::subject(),
            &cancellation,
        )
        .await
        .unwrap_err();
    assert_eq!(unsent.recovery(), RecoveryDisposition::None);
    active.await??;
    assert_eq!(authority.0.load(Ordering::SeqCst), 1);
    assert_eq!(h.snapshots().await[0].lifecycle, PluginLifecycle::Ready);
    h.stop(&id).await?;
    h.start(&id, v).await?;
    h.stop(&id).await?;
    println!("actual framed process, queued cancellation, authority timing, stop/start passed");
    for mode in ["block", "wrong"] {
        let (h, id, v, _) = host(base, mode).await?;
        let cancellation = HostCancellation::new();
        let c2 = cancellation.clone();
        let h2 = h.clone();
        let id2 = id.clone();
        let before = Instant::now();
        let call = tokio::spawn(async move {
            h2.invoke(
                &id2,
                RequestId::new("blocked").unwrap(),
                "example.inspect",
                payload(true),
                &crate::subject(),
                &c2,
            )
            .await
        });
        if mode == "block" {
            tokio::time::sleep(Duration::from_millis(80)).await;
            let _ = cancellation.cancel();
        }
        let e = call.await?.unwrap_err();
        assert_eq!(e.recovery(), RecoveryDisposition::RestartPlugin);
        assert!(before.elapsed() < Duration::from_secs(3));
        assert_eq!(h.snapshots().await[0].lifecycle, PluginLifecycle::Failed);
        let _ = h.stop(&id).await;
        assert!(h.snapshots().await.is_empty());
        h.start(&id, v).await?;
        if mode == "block" {
            let h2 = h.clone();
            let id2 = id.clone();
            let active = tokio::spawn(async move {
                h2.invoke(
                    &id2,
                    RequestId::new("stop-active").unwrap(),
                    "example.inspect",
                    payload(true),
                    &crate::subject(),
                    &HostCancellation::new(),
                )
                .await
            });
            tokio::time::sleep(Duration::from_millis(60)).await;
            let before = Instant::now();
            let (first_stop, second_stop) = tokio::join!(h.stop(&id), h.stop(&id));
            first_stop?;
            second_stop?;
            let _ = active.await?;
            assert!(before.elapsed() < Duration::from_secs(3));
            let pid = fs::read_to_string(base.join("block/example/descendant.pid"))?;
            let status = fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
            assert!(status.is_err() || status.unwrap().contains(") Z"));
        } else {
            h.stop(&id).await?;
        }
    }
    let (h, id, v, _) = host(base, "bad-shutdown").await?;
    let (a, b) = tokio::join!(h.stop(&id), h.stop(&id));
    assert!(a.is_err() || b.is_err());
    assert!(h.snapshots().await.is_empty());
    h.start(&id, v).await?;
    h.invoke(
        &id,
        RequestId::new("after-bad-shutdown").unwrap(),
        "example.inspect",
        payload(false),
        &crate::subject(),
        &HostCancellation::new(),
    )
    .await?;
    assert_eq!(h.snapshots().await[0].lifecycle, PluginLifecycle::Ready);
    let _ = h.stop(&id).await;
    println!(
        "malformed shutdown acknowledgement, concurrent stop completion and fresh-instance recovery passed"
    );
    let (h, id, _, _) = host(base, "block").await?;
    let before = Instant::now();
    let error = h
        .invoke(
            &id,
            RequestId::new("deadline").unwrap(),
            "example.inspect",
            payload(true),
            &crate::subject(),
            &HostCancellation::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.class(), HostFailureClass::Timeout);
    assert!(before.elapsed() < Duration::from_secs(3));
    let _ = h.stop(&id).await;
    println!(
        "blocked-pipe cancellation/deadline, malformed frame, active stop, descendant cleanup, explicit restart, UTF-8 diagnostics passed"
    );
    Ok(())
}
