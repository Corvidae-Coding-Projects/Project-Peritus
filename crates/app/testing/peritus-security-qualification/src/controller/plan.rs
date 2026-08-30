//! Closed mapping from every H0 probe to independently executable candidate checks.

use crate::ProbeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceCheck {
    MigrationRecovery,
    UnsafeInventory,
    TcbInventory,
    ThreatInventory,
    ControlInventory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeCheck {
    LinuxBubblewrap,
    MacosSeatbelt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CommandCheck {
    pub(super) label: &'static str,
    pub(super) arguments: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Check {
    Command(CommandCheck),
    Native(NativeCheck),
    Source(SourceCheck),
}

pub(super) struct ProbePlan {
    pub(super) checks: Vec<Check>,
    pub(super) native_sandbox: bool,
}

pub(super) fn for_probe(probe: ProbeId) -> ProbePlan {
    use ProbeId as P;

    let checks = match probe {
        P::RepositoryTraversal
        | P::SymlinkRace
        | P::SubmoduleEscape
        | P::WorktreeEscape
        | P::CaseAliasCollision
        | P::DevicePath
        | P::ShellInjection
        | P::PoisonedRepositoryInstructions
        | P::OversizedOutput
        | P::TerminalEscape => repository(probe),
        P::SecretExfiltration
        | P::LinuxSandboxCapabilities
        | P::MacosSandboxCapabilities
        | P::WindowsSandboxCapabilities
        | P::SandboxEscape
        | P::NetworkDefaultDeny
        | P::PluginCapabilityScope
        | P::McpCapabilityScope => sandbox_and_extensions(probe),
        P::ReviewerReadOnly
        | P::FixerCannotApprove
        | P::WriterCannotWaive
        | P::CandidateMutationInvalidation => authority(probe),
        P::SealedAnswerDenial
        | P::EvaluatorMutationDenial
        | P::ProfileMutationDenial
        | P::SelfPromotionDenial
        | P::EvolutionCampaignIsolation
        | P::PromotionGateBinding
        | P::AtomicRollbackHistory => evolution(probe),
        P::EvidenceCitation
        | P::InfrastructureFailureTaxonomy
        | P::SecretRedaction
        | P::DependencyReproducibility
        | P::ReleaseSignatureSbom
        | P::MigrationRecoveryDocumentation => evidence_and_supply_chain(probe),
        P::UnsafeInventory
        | P::TcbInventory
        | P::NoQuarantinedOrPlaceholderProduction
        | P::FindingLifecycle
        | P::CancellationAndTreeCleanup
        | P::ThreatInventory
        | P::ControlInventory => governance(probe),
    };
    ProbePlan {
        checks,
        native_sandbox: matches!(
            probe,
            P::SecretExfiltration
                | P::LinuxSandboxCapabilities
                | P::MacosSandboxCapabilities
                | P::WindowsSandboxCapabilities
                | P::SandboxEscape
                | P::NetworkDefaultDeny
        ),
    }
}

fn repository(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::RepositoryTraversal => vec![test("peritus-tools-fs", "filesystem_tools", None)],
        P::SymlinkRace => vec![test(
            "peritus-tools-fs",
            "filesystem_tools",
            Some("immutable_inspection_refuses_symlink_traversal"),
        )],
        P::SubmoduleEscape => vec![test("peritus-git", "git_safety", None)],
        P::WorktreeEscape => vec![test("peritus-git", "git_lifecycle", None)],
        P::CaseAliasCollision => vec![test(
            "peritus-sandbox-linux",
            "contracts",
            Some("protected_root_alias_cannot_escape_the_workspace"),
        )],
        P::DevicePath => vec![test(
            "peritus-sandbox-windows",
            "contracts",
            Some("paths_reject_device_ads_reserved_and_escape_forms"),
        )],
        P::ShellInjection => vec![test(
            "peritus-tools-shell",
            "execution_integration",
            Some("shell_exec_runs_literal_argv_accepts_stdin_and_publishes_output_artifacts"),
        )],
        P::PoisonedRepositoryInstructions => {
            vec![test("peritus-agent", "runtime_context", None)]
        }
        P::OversizedOutput => vec![test(
            "peritus-plugin-host",
            "process_host",
            Some("host_output_ceiling_and_duplicate_start_are_enforced"),
        )],
        P::TerminalEscape => vec![test("peritus-approval", "rendering", None)],
        _ => unreachable!("repository plan called for another probe"),
    }
}

fn sandbox_and_extensions(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::SecretExfiltration => vec![
            native(NativeCheck::LinuxBubblewrap),
            test("peritus-secrets", "secrets", None),
            test(
                "peritus-sandbox-linux",
                "native_enforcement",
                Some("helper_inherits_only_bound_secret_handle_and_installs_exact_environment"),
            ),
        ],
        P::LinuxSandboxCapabilities | P::SandboxEscape => {
            vec![
                native(NativeCheck::LinuxBubblewrap),
                test("peritus-sandbox-linux", "native_enforcement", None),
            ]
        }
        P::MacosSandboxCapabilities => {
            vec![
                native(NativeCheck::MacosSeatbelt),
                test("peritus-sandbox-macos", "native_probe", None),
            ]
        }
        P::WindowsSandboxCapabilities => vec![
            test("peritus-sandbox-windows", "native_enforcement", None),
            test("peritus-sandbox-windows", "windows_conformance", None),
        ],
        P::NetworkDefaultDeny => vec![
            native(NativeCheck::LinuxBubblewrap),
            test("peritus-network", "network", None),
            test("peritus-sandbox-linux", "native_enforcement", None),
        ],
        P::PluginCapabilityScope => vec![test("peritus-plugin-host", "process_host", None)],
        P::McpCapabilityScope => vec![test("peritus-mcp", "server", None)],
        _ => unreachable!("sandbox plan called for another probe"),
    }
}

fn authority(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::ReviewerReadOnly => vec![
            test("peritus-role", "role_matrix", None),
            test("peritus-review", "domain_engine", None),
        ],
        P::FixerCannotApprove => vec![test(
            "peritus-product-runner",
            "production_composition",
            Some("fixer_cannot_erase_a_finding_without_fresh_reviewer_confirmation"),
        )],
        P::WriterCannotWaive => vec![
            test("peritus-role", "role_matrix", None),
            test("peritus-review", "domain_transitions", None),
        ],
        P::CandidateMutationInvalidation => vec![test("peritus-evidence", "freshness", None)],
        _ => unreachable!("authority plan called for another probe"),
    }
}

fn evolution(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::SealedAnswerDenial | P::ProfileMutationDenial => {
            vec![test("peritus-eval", "dataset_profile", None)]
        }
        P::EvaluatorMutationDenial => vec![test("peritus-eval", "execution_isolation", None)],
        P::SelfPromotionDenial | P::PromotionGateBinding => {
            vec![test("peritus-evolution", "promotion_authority", None)]
        }
        P::EvolutionCampaignIsolation => {
            vec![test("peritus-evolution", "production_conformance", None)]
        }
        P::AtomicRollbackHistory => vec![
            test("peritus-evolution", "durability_restart", None),
            test("peritus-evolution", "publication_integration", None),
        ],
        _ => unreachable!("evolution plan called for another probe"),
    }
}

fn evidence_and_supply_chain(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::EvidenceCitation => vec![
            test("peritus-evidence", "admission", None),
            test("peritus-trace", "domain_projection", None),
        ],
        P::InfrastructureFailureTaxonomy => {
            vec![test("peritus-eval", "execution_isolation", None)]
        }
        P::SecretRedaction => vec![
            test("peritus-secrets", "secrets", None),
            test("peritus-trace", "redaction_codec", None),
        ],
        P::DependencyReproducibility => vec![xtask("reproducibility-check")],
        P::ReleaseSignatureSbom => {
            vec![test("peritus-release-artifacts", "release_contracts", None)]
        }
        P::MigrationRecoveryDocumentation => vec![source(SourceCheck::MigrationRecovery)],
        _ => unreachable!("evidence plan called for another probe"),
    }
}

fn governance(probe: ProbeId) -> Vec<Check> {
    use ProbeId as P;

    match probe {
        P::UnsafeInventory => vec![
            source(SourceCheck::UnsafeInventory),
            cargo(
                "workspace-strict-clippy",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
            ),
        ],
        P::TcbInventory => vec![source(SourceCheck::TcbInventory), xtask("verify-trust")],
        P::NoQuarantinedOrPlaceholderProduction => vec![xtask("source-layout-check")],
        P::FindingLifecycle => vec![
            test("peritus-security-policy", "policy", None),
            test("peritus-review", "domain_engine", None),
        ],
        P::CancellationAndTreeCleanup => {
            vec![test("peritus-process", "process_integration", None)]
        }
        P::ThreatInventory => vec![source(SourceCheck::ThreatInventory)],
        P::ControlInventory => vec![source(SourceCheck::ControlInventory)],
        _ => unreachable!("governance plan called for another probe"),
    }
}

const fn source(check: SourceCheck) -> Check {
    Check::Source(check)
}

const fn native(check: NativeCheck) -> Check {
    Check::Native(check)
}

fn xtask(command: &'static str) -> Check {
    cargo("workspace-policy", &["run", "--locked", "--package", "xtask", "--", command])
}

fn test(package: &'static str, target: &'static str, filter: Option<&'static str>) -> Check {
    let mut arguments = vec![
        "test".to_owned(),
        "--locked".to_owned(),
        "--package".to_owned(),
        package.to_owned(),
        "--test".to_owned(),
        target.to_owned(),
    ];
    if let Some(filter) = filter {
        arguments.extend([filter.to_owned(), "--".to_owned(), "--exact".to_owned()]);
    }
    Check::Command(CommandCheck { label: "candidate-regression", arguments })
}

fn cargo(label: &'static str, arguments: &[&str]) -> Check {
    Check::Command(CommandCheck {
        label,
        arguments: arguments.iter().map(|value| (*value).to_owned()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProbeSpec;

    #[test]
    fn every_catalog_probe_has_a_nonempty_closed_plan() {
        for spec in ProbeSpec::h0_production() {
            assert!(
                !for_probe(spec.id()).checks.is_empty(),
                "missing plan for {}",
                spec.id().as_str()
            );
        }
    }
}
