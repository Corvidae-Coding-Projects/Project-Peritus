//! Production plugin-host adapter for the reusable A2 G3 contract.

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, PluginConformanceError,
    PluginConformanceFixture, PluginConformanceObservation, PluginConformanceSubject,
    PluginDisposition, PluginScenario, ReportText, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, plugin_suite,
};

use super::*;

struct ProductionSubject;

impl PluginConformanceSubject for ProductionSubject {
    fn exercise(
        &mut self,
        fixture: &PluginConformanceFixture,
    ) -> Result<PluginConformanceObservation, PluginConformanceError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PluginConformanceError::Infrastructure)?
            .block_on(observe(fixture.scenario()))
    }
}

async fn observe(
    scenario: PluginScenario,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let fixture = Fixture::new();
    match scenario {
        PluginScenario::CanonicalManifest => canonical_manifest(&fixture),
        PluginScenario::TrustRequired => trust_required(&fixture).await,
        PluginScenario::AuthorityDenied => authority_denied(&fixture).await,
        PluginScenario::Lifecycle => lifecycle(&fixture).await,
        PluginScenario::Quota => {
            observe_failure(&fixture, "oversize", HostFailureClass::Quota).await
        }
        PluginScenario::Cancellation => observe_cancellation(&fixture).await,
        PluginScenario::CrashIsolation => {
            observe_failure(&fixture, "crash", HostFailureClass::Infrastructure).await
        }
    }
}

fn canonical_manifest(
    fixture: &Fixture,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let discovered = fixture
        .catalog
        .get(&fixture.id, fixture.version)
        .ok_or(PluginConformanceError::Infrastructure)?;
    let repeated = discover(std::slice::from_ref(&fixture.root), DiscoveryLimits::PRODUCTION)
        .map_err(|_| PluginConformanceError::Infrastructure)?;
    let same = repeated.get(&fixture.id, fixture.version).is_some_and(|again| {
        again.manifest_digest() == discovered.manifest_digest()
            && again.artifact_sha256() == discovered.artifact_sha256()
    });
    Ok(observation(PluginDisposition::Succeeded, same))
}

async fn trust_required(
    fixture: &Fixture,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let hosted = PluginHost::new(
        Fixture::config(4_096),
        fixture.catalog.clone(),
        Arc::new(Allow),
        Arc::new(DigestTrustStore::new()),
    );
    let denied = hosted
        .start(&fixture.id, fixture.version)
        .await
        .is_err_and(|error| error.class() == HostFailureClass::Trust);
    let empty = hosted.snapshots().await.is_empty();
    Ok(PluginConformanceObservation::new(
        if denied { PluginDisposition::Rejected } else { PluginDisposition::Failed },
        true,
        denied,
        false,
        invocation_count(fixture),
        true,
        empty,
        empty,
        true,
        denied,
    ))
}

async fn authority_denied(
    fixture: &Fixture,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let hosted = host(fixture, Arc::new(Deny), 4_096);
    hosted
        .start(&fixture.id, fixture.version)
        .await
        .map_err(|_| PluginConformanceError::Infrastructure)?;
    let denied = hosted
        .invoke(
            &fixture.id,
            request_id("conformance-denied")?,
            "fs.read",
            payload(json(r#"{"path":"README.md"}"#)),
            &subject(),
            &HostCancellation::new(),
        )
        .await
        .is_err_and(|error| error.class() == HostFailureClass::Authorization);
    hosted.stop(&fixture.id).await.map_err(|_| PluginConformanceError::Infrastructure)?;
    Ok(PluginConformanceObservation::new(
        if denied { PluginDisposition::Rejected } else { PluginDisposition::Failed },
        true,
        true,
        denied,
        invocation_count(fixture),
        true,
        true,
        true,
        true,
        denied,
    ))
}

async fn lifecycle(
    fixture: &Fixture,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let hosted = host(fixture, Arc::new(Allow), 4_096);
    hosted
        .start(&fixture.id, fixture.version)
        .await
        .map_err(|_| PluginConformanceError::Infrastructure)?;
    let succeeded = hosted
        .invoke(
            &fixture.id,
            request_id("conformance-lifecycle")?,
            "fs.read",
            payload(json(r#"{"path":"README.md"}"#)),
            &subject(),
            &HostCancellation::new(),
        )
        .await
        .is_ok_and(|result| matches!(result, PluginInvocationResult::Succeeded { .. }));
    hosted.stop(&fixture.id).await.map_err(|_| PluginConformanceError::Infrastructure)?;
    let released = hosted.snapshots().await.is_empty();
    Ok(PluginConformanceObservation::new(
        if succeeded { PluginDisposition::Succeeded } else { PluginDisposition::Failed },
        true,
        true,
        true,
        invocation_count(fixture),
        true,
        released,
        released,
        true,
        !succeeded,
    ))
}

async fn observe_failure(
    fixture: &Fixture,
    mode: &'static str,
    expected: HostFailureClass,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let output = if mode == "oversize" { 128 } else { 4_096 };
    let hosted = host(fixture, Arc::new(Allow), output);
    hosted
        .start(&fixture.id, fixture.version)
        .await
        .map_err(|_| PluginConformanceError::Infrastructure)?;
    let failed = hosted
        .invoke(
            &fixture.id,
            request_id("conformance-failure")?,
            "fs.read",
            payload(json(&format!(r#"{{"mode":"{mode}"}}"#))),
            &subject(),
            &HostCancellation::new(),
        )
        .await
        .is_err_and(|error| error.class() == expected);
    let host_alive = hosted.snapshots().await.len() == 1;
    Ok(PluginConformanceObservation::new(
        PluginDisposition::Failed,
        true,
        true,
        true,
        invocation_count(fixture),
        mode == "oversize" && failed,
        failed,
        failed,
        host_alive,
        failed,
    ))
}

async fn observe_cancellation(
    fixture: &Fixture,
) -> Result<PluginConformanceObservation, PluginConformanceError> {
    let hosted = Arc::new(host(fixture, Arc::new(Allow), 4_096));
    hosted
        .start(&fixture.id, fixture.version)
        .await
        .map_err(|_| PluginConformanceError::Infrastructure)?;
    let cancellation = HostCancellation::new();
    let invocation = {
        let hosted = Arc::clone(&hosted);
        let id = fixture.id.clone();
        let cancellation = cancellation.clone();
        tokio::spawn(async move {
            hosted
                .invoke(
                    &id,
                    request_id("conformance-cancel")?,
                    "fs.read",
                    payload(json(r#"{"mode":"sleep"}"#)),
                    &subject(),
                    &cancellation,
                )
                .await
                .map_err(|_| PluginConformanceError::Infrastructure)
        })
    };
    wait_for_file(&fixture.log()).await;
    let _ = cancellation.cancel();
    let cancelled = tokio::time::timeout(Duration::from_secs(2), invocation)
        .await
        .map_err(|_| PluginConformanceError::Infrastructure)?
        .map_err(|_| PluginConformanceError::Infrastructure)?
        .is_err();
    Ok(PluginConformanceObservation::new(
        if cancelled { PluginDisposition::Cancelled } else { PluginDisposition::Failed },
        true,
        true,
        true,
        invocation_count(fixture),
        true,
        cancelled,
        cancelled,
        true,
        cancelled,
    ))
}

const fn observation(
    disposition: PluginDisposition,
    canonical_identity: bool,
) -> PluginConformanceObservation {
    PluginConformanceObservation::new(
        disposition,
        canonical_identity,
        false,
        false,
        0,
        true,
        true,
        true,
        true,
        false,
    )
}

fn request_id(value: &str) -> Result<RequestId, PluginConformanceError> {
    RequestId::new(value).map_err(|_| PluginConformanceError::Infrastructure)
}

fn invocation_count(fixture: &Fixture) -> u64 {
    fs::read_to_string(fixture.log())
        .map_or(0, |log| u64::try_from(log.lines().count()).unwrap_or(u64::MAX))
}

struct Factory(SubjectDescriptor);

impl Factory {
    fn new() -> Self {
        Self(SubjectDescriptor::new(
            ReportText::new("peritus-plugin-host").expect("subject name"),
            ReportText::new("production G3 process host adapter").expect("implementation"),
        ))
    }
}

impl SubjectFactory<ProductionSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.0
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionSubject, SubjectFailure>> {
        Box::pin(async { Ok(ProductionSubject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_host_passes_all_seven_a2_plugin_cases() {
    let report =
        block_on(ConformanceRunner::run(&plugin_suite::<ProductionSubject>(), &Factory::new()));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 7);
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}
