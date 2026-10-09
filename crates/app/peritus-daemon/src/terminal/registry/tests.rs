//! Real preview lifecycle regressions for disconnected terminal capacity.

use std::{
    io::Write as _,
    time::{Duration, Instant},
};

use peritus_app_protocol::{RequestId, TerminalAttachmentId, TerminalBinding};
use peritus_process::{ProcessControl, ProcessStore};
use peritus_product_runner::{CommandRuntime, PreviewCommand, PreviewLaunch};
use peritus_types::{ActorId, RunId, SessionId};

use super::{TerminalBridgeErrorKind, TerminalRegistry, TerminalRegistryLimits};

#[test]
#[ignore = "subprocess fixture; invoked by terminal registry lifecycle tests"]
fn terminal_capacity_fixture() {
    println!("REGISTRY READY");
    std::io::stdout().flush().expect("readiness");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).expect("finish input");
    assert_eq!(line.trim(), "finish");
}

struct Fixture {
    runtime: CommandRuntime,
    workspace: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let workspace = tempfile::tempdir().expect("workspace");
        let state = tempfile::tempdir().expect("state");
        let processes = ProcessStore::open(state.path().join("processes"), workspace.path())
            .expect("process store");
        let runtime = CommandRuntime::open(
            state.path().join("router"),
            workspace.path(),
            RunId::new([57; 16]).expect("run"),
            processes,
        )
        .expect("runtime");
        Self { runtime, workspace, _state: state }
    }

    fn launch(&self, key: &str) -> (PreviewLaunch, ProcessControl) {
        let command = PreviewCommand::new(
            std::env::current_exe().expect("test executable").to_string_lossy().into_owned(),
            [
                "--ignored",
                "--exact",
                "terminal::registry::tests::terminal_capacity_fixture",
                "--nocapture",
            ]
            .map(str::to_owned)
            .to_vec(),
            self.workspace.path().to_path_buf(),
            Duration::from_secs(20),
            true,
            24,
            80,
            key.to_owned(),
            Vec::new(),
        )
        .expect("command");
        let launch = self.runtime.launch_preview(&command).expect("launch");
        let control = self.runtime.preview_terminal(&launch).expect("terminal").control();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let observation = self.runtime.observe_preview(&launch).expect("observe readiness");
            if observation.stdout().contains("REGISTRY READY") {
                break;
            }
            assert!(Instant::now() < deadline, "fixture never became ready: {observation:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
        (launch, control)
    }

    fn register(
        &self,
        registry: &TerminalRegistry,
        launch: &PreviewLaunch,
    ) -> Result<(), super::TerminalBridgeError> {
        registry.register_preview(
            actor(),
            session(),
            self.runtime.preview_terminal(launch).expect("lease"),
        )
    }
}

fn actor() -> ActorId {
    ActorId::new([57; 16]).expect("actor")
}
fn session() -> SessionId {
    SessionId::new([57; 16]).expect("session")
}

fn finish(control: &ProcessControl) {
    control.write_stdin(b"finish\n".to_vec()).expect("finish input");
    wait_for_completion(control);
}

fn wait_for_completion(control: &ProcessControl) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while control.terminal_result().is_none() {
        assert!(Instant::now() < deadline, "fixture did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn completed_disconnected_previews_release_registry_capacity() {
    let fixture = Fixture::new();
    let mut limits = TerminalRegistryLimits::PRODUCTION;
    limits.maximum_processes = 1;
    let registry = TerminalRegistry::new(limits).expect("registry");
    let (first, first_control) = fixture.launch("first");
    fixture.register(&registry, &first).expect("first registration");
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([57; 16]).expect("attachment"),
        first.process_id(),
        RequestId::new([57; 16]).expect("request"),
    );
    registry.attach(actor(), session(), binding, 8192).expect("attach");
    registry.release_attachments(actor(), session(), &[binding]);
    let (second, second_control) = fixture.launch("second");
    assert_eq!(
        fixture.register(&registry, &second).expect_err("live owner retains capacity").kind(),
        TerminalBridgeErrorKind::Capacity
    );
    assert!(first_control.terminal_result().is_none(), "disconnect must not kill the process");
    finish(&first_control);
    fixture.register(&registry, &second).expect("finished disconnected owner releases capacity");
    assert_eq!(registry.counts(), (1, 0));
    finish(&second_control);
    assert_eq!(registry.shutdown().expect("cleanup"), 1);
}

#[test]
fn completed_attached_previews_keep_pending_delivery_until_disconnect() {
    let fixture = Fixture::new();
    let mut limits = TerminalRegistryLimits::PRODUCTION;
    limits.maximum_processes = 1;
    let registry = TerminalRegistry::new(limits).expect("registry");
    let (first, first_control) = fixture.launch("first");
    fixture.register(&registry, &first).expect("first registration");
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([58; 16]).expect("attachment"),
        first.process_id(),
        RequestId::new([58; 16]).expect("request"),
    );
    registry.attach(actor(), session(), binding, 8192).expect("attach");
    finish(&first_control);
    let (second, second_control) = fixture.launch("second");
    assert_eq!(
        fixture.register(&registry, &second).expect_err("retain attached output").kind(),
        TerminalBridgeErrorKind::Capacity
    );
    assert_eq!(registry.counts(), (1, 1));
    registry.release_attachments(actor(), session(), &[binding]);
    fixture.register(&registry, &second).expect("released delivery can be retired");
    finish(&second_control);
    assert_eq!(registry.shutdown().expect("cleanup"), 1);
}

#[test]
fn repeated_detach_and_reattach_preserves_receipts_without_consuming_live_slots() {
    use peritus_app_protocol::{CorrelationId, TerminalDetach, TerminalTransitionDisposition};
    let fixture = Fixture::new();
    let registry = TerminalRegistry::new(TerminalRegistryLimits::PRODUCTION).expect("registry");
    let (launch, control) = fixture.launch("reattach");
    fixture.register(&registry, &launch).expect("register");
    let mut receipts = Vec::new();
    for id in 1..=12 {
        let binding = TerminalBinding::new(
            TerminalAttachmentId::new([id; 16]).expect("attachment"),
            launch.process_id(),
            RequestId::new([id; 16]).expect("request"),
        );
        registry.attach(actor(), session(), binding, 8192).expect("reattach after detach");
        let detach =
            TerminalDetach::new(binding, CorrelationId::new([id; 16]).expect("correlation"));
        assert_eq!(
            registry.detach(actor(), session(), detach).expect("detach"),
            TerminalTransitionDisposition::Applied
        );
        receipts.push(detach);
    }
    for detach in receipts {
        assert_eq!(
            registry.detach(actor(), session(), detach).expect("repeat exact fact"),
            TerminalTransitionDisposition::Repeated
        );
        assert!(
            registry.detach(ActorId::new([99; 16]).expect("stranger"), session(), detach).is_err()
        );
        let conflict =
            TerminalDetach::new(detach.binding(), CorrelationId::new([99; 16]).expect("conflict"));
        assert!(registry.detach(actor(), session(), conflict).is_err());
        assert!(
            registry.attach(actor(), session(), detach.binding(), 8192).is_err(),
            "receipt identity cannot be reused"
        );
    }
    assert_eq!(registry.counts(), (1, 0));
    let next_session = SessionId::new([58; 16]).expect("reconnected");
    registry
        .register_preview(
            actor(),
            next_session,
            fixture.runtime.preview_terminal(&launch).expect("lease"),
        )
        .expect("explicit detach releases session ownership");
    finish(&control);
    assert_eq!(registry.shutdown().expect("cleanup"), 1);
}

#[test]
fn cancelled_terminal_receipt_survives_owner_retirement() {
    use peritus_app_protocol::{
        CorrelationId, TerminalCancellation, TerminalDetach, TerminalTransitionDisposition,
    };
    let fixture = Fixture::new();
    let registry = TerminalRegistry::new(TerminalRegistryLimits::PRODUCTION).expect("registry");
    let (launch, control) = fixture.launch("cancel");
    fixture.register(&registry, &launch).expect("register");
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([57; 16]).expect("attachment"),
        launch.process_id(),
        RequestId::new([57; 16]).expect("request"),
    );
    registry.attach(actor(), session(), binding, 8192).expect("attach");
    let correlation = CorrelationId::new([57; 16]).expect("correlation");
    let cancellation = TerminalCancellation::new(binding, correlation);
    assert_eq!(
        registry.cancel(actor(), session(), cancellation).expect("cancel"),
        TerminalTransitionDisposition::Applied
    );
    assert_eq!(registry.counts(), (1, 0));
    wait_for_completion(&control);
    assert!(registry.retire(launch.process_id()).expect("retire cancelled owner"));
    assert_eq!(registry.counts(), (0, 0));
    assert_eq!(
        registry
            .cancel(actor(), session(), cancellation)
            .expect("retry cancellation after retirement"),
        TerminalTransitionDisposition::Repeated
    );
    assert!(
        registry.detach(actor(), session(), TerminalDetach::new(binding, correlation)).is_err()
    );
    assert!(
        registry
            .cancel(actor(), SessionId::new([99; 16]).expect("stranger session"), cancellation)
            .is_err()
    );
    assert_eq!(registry.shutdown().expect("cleanup"), 0);
}
