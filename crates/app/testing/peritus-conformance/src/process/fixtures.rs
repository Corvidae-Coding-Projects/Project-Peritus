//! Fixed process inputs shared by the executable cases.

use super::{
    ProcessAuthorizationDrift, ProcessConformanceFixture, ProcessEnvironmentBinding, ProcessIoMode,
    ProcessScenario,
};

const ENVIRONMENT: &[ProcessEnvironmentBinding] = &[
    ProcessEnvironmentBinding::new("PERITUS_ALPHA", "literal value"),
    ProcessEnvironmentBinding::new("PERITUS_BETA", "$NOT_EXPANDED"),
];
const LITERAL_ARGUMENTS: &[&str] =
    &["space value", "\"quoted\"", "*.rs", "$HOME", ">out", "a;b", "$(touch nope)"];

pub(super) const AUTHORITY_DRIFTS: [ProcessAuthorizationDrift; 17] = [
    ProcessAuthorizationDrift::Action,
    ProcessAuthorizationDrift::IntentPayload,
    ProcessAuthorizationDrift::MediaType,
    ProcessAuthorizationDrift::OwnerLineage,
    ProcessAuthorizationDrift::ActorRole,
    ProcessAuthorizationDrift::Environment,
    ProcessAuthorizationDrift::Resource,
    ProcessAuthorizationDrift::Capability,
    ProcessAuthorizationDrift::Budget,
    ProcessAuthorizationDrift::OperationClass,
    ProcessAuthorizationDrift::Dispatch,
    ProcessAuthorizationDrift::Revision,
    ProcessAuthorizationDrift::Generation,
    ProcessAuthorizationDrift::HolderLease,
    ProcessAuthorizationDrift::AuthorityTime,
    ProcessAuthorizationDrift::SandboxDigest,
    ProcessAuthorizationDrift::BackendPreparation,
];

pub(super) const fn fixture(scenario: ProcessScenario) -> ProcessConformanceFixture {
    let (arguments, stdin, mode, output_limit, descendant_depth) = match scenario {
        ProcessScenario::LiteralInvocation => {
            (LITERAL_ARGUMENTS, b"" as &[u8], ProcessIoMode::Pipes, 64, 0)
        }
        ProcessScenario::PipeStreaming => {
            (&["pipe"] as &[&str], b"pipe-input" as &[u8], ProcessIoMode::Pipes, 64, 0)
        }
        ProcessScenario::PtyStreaming => {
            (&["pty"] as &[&str], b"pty-input" as &[u8], ProcessIoMode::Pty, 64, 0)
        }
        ProcessScenario::OutputBound => {
            (&["output"] as &[&str], b"" as &[u8], ProcessIoMode::Pipes, 4, 0)
        }
        ProcessScenario::TreeCleanup => {
            (&["tree"] as &[&str], b"" as &[u8], ProcessIoMode::Pipes, 64, 2)
        }
        _ => (&["control"] as &[&str], b"" as &[u8], ProcessIoMode::Pipes, 64, 0),
    };
    ProcessConformanceFixture::new(
        scenario,
        "peritus-conformance-helper",
        arguments,
        "/checked/workspace",
        ENVIRONMENT,
        stdin,
        mode,
        output_limit,
        descendant_depth,
    )
}
