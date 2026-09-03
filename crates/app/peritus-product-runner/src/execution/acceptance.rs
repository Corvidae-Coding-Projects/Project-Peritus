//! Delivery-scope-specific acceptance evidence and decisions.

use peritus_orchestrator::{ProductionDecision, ProductionRunCoordinator};
use peritus_review::ProductFindingLedger;

use super::ProductDeliveryScope;
use crate::{
    delivery_requirement::ExternalEffectRequirement,
    developer_tools::{CommandPurpose, SuccessfulCommand},
    gates,
};

pub(super) struct ExternalEffectEvidence {
    effects: usize,
    verifications: usize,
    verification_after_effect: bool,
}

impl ExternalEffectEvidence {
    pub(super) fn from_commands(commands: &[SuccessfulCommand]) -> Self {
        let mut effects = 0;
        let mut verifications = 0;
        let mut effect_seen = false;
        let mut verification_after_effect = false;
        for command in commands {
            match command.purpose {
                CommandPurpose::ExternalEffect => {
                    effects += 1;
                    effect_seen = true;
                }
                CommandPurpose::Verification => {
                    verifications += 1;
                    verification_after_effect |= effect_seen;
                }
            }
        }
        Self { effects, verifications, verification_after_effect }
    }

    const fn complete(&self) -> bool {
        self.effects > 0 && self.verifications > 0 && self.verification_after_effect
    }

    pub(super) fn append_report(
        &self,
        report: &mut String,
        requirement: ExternalEffectRequirement,
    ) {
        use core::fmt::Write as _;

        let _ = write!(
            report,
            "\nAuthorized external-effect evidence:\n  required by the operational request: {}\n  successful effect commands: {}\n  successful fresh verification commands: {}\nExternal-effect evidence: {}\n",
            if requirement.is_required() { "yes" } else { "no" },
            self.effects,
            self.verifications,
            if self.complete() { "READY FOR INDEPENDENT REVIEW" } else { "INCOMPLETE" },
        );
    }
}

pub(super) fn qualification_ready(
    scope: ProductDeliveryScope,
    requirement: ExternalEffectRequirement,
    gates: &gates::GateReport,
    commands: &[SuccessfulCommand],
) -> bool {
    let workspace_ready = gates.report.passed()
        || (scope.allows_external_effects() && gates.report.changed_paths().is_empty());
    let external_ready = !external_evidence_required(scope, requirement, gates)
        || ExternalEffectEvidence::from_commands(commands).complete();
    workspace_ready && external_ready
}

pub(super) fn decide(
    scope: ProductDeliveryScope,
    requirement: ExternalEffectRequirement,
    coordinator: &ProductionRunCoordinator,
    gates: &gates::GateReport,
    findings: &ProductFindingLedger,
    commands: &[SuccessfulCommand],
) -> ProductionDecision {
    let evidence = ExternalEffectEvidence::from_commands(commands);
    if external_evidence_required(scope, requirement, gates) && !evidence.complete() {
        coordinator.decide_external_effects(false, findings)
    } else if scope.allows_external_effects() && gates.report.changed_paths().is_empty() {
        coordinator.decide_external_effects(evidence.complete(), findings)
    } else {
        coordinator.decide(&gates.report, findings)
    }
}

pub(super) fn successful_command_lines(
    scope: ProductDeliveryScope,
    requirement: ExternalEffectRequirement,
    gates: &gates::GateReport,
    commands: &[SuccessfulCommand],
) -> Vec<String> {
    if scope.allows_external_effects() && gates.report.changed_paths().is_empty() {
        commands.iter().map(|command| command.command.clone()).collect()
    } else {
        let mut lines: Vec<String> =
            gates.report.records().iter().map(|record| record.command.clone()).collect();
        if requirement.is_required() {
            lines.extend(commands.iter().map(|command| command.command.clone()));
        }
        lines
    }
}

fn external_evidence_required(
    scope: ProductDeliveryScope,
    requirement: ExternalEffectRequirement,
    gates: &gates::GateReport,
) -> bool {
    scope.allows_external_effects()
        && (requirement.is_required() || gates.report.changed_paths().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_gates::{GateExecutionRecord, TargetGatePlan, TargetGateReport};

    fn command(purpose: CommandPurpose) -> SuccessfulCommand {
        SuccessfulCommand { command: "run_command {}".to_owned(), purpose }
    }

    #[test]
    fn external_effect_evidence_needs_an_action_and_a_fresh_verification() {
        assert!(!ExternalEffectEvidence::from_commands(&[]).complete());
        assert!(
            !ExternalEffectEvidence::from_commands(&[command(CommandPurpose::ExternalEffect,)])
                .complete()
        );
        assert!(
            !ExternalEffectEvidence::from_commands(&[
                command(CommandPurpose::Verification),
                command(CommandPurpose::ExternalEffect),
            ])
            .complete()
        );
        assert!(
            ExternalEffectEvidence::from_commands(&[
                command(CommandPurpose::ExternalEffect),
                command(CommandPurpose::Verification),
            ])
            .complete()
        );
    }

    #[test]
    fn external_evidence_never_weakens_the_default_workspace_scope() {
        let root = tempfile::tempdir().expect("root");
        let plan = TargetGatePlan::discover(root.path(), Vec::new()).expect("empty plan");
        let report = TargetGateReport::from_execution(&plan, Vec::<GateExecutionRecord>::new());
        let gates = gates::GateReport { report, output: String::new() };
        let commands =
            [command(CommandPurpose::ExternalEffect), command(CommandPurpose::Verification)];
        let coordinator = ProductionRunCoordinator::new(2).expect("coordinator");
        let findings = ProductFindingLedger::new();

        assert_eq!(
            decide(
                ProductDeliveryScope::WorkspaceChanges,
                ExternalEffectRequirement::Optional,
                &coordinator,
                &gates,
                &findings,
                &commands,
            ),
            ProductionDecision::Fix,
        );
        assert_eq!(
            decide(
                ProductDeliveryScope::AuthorizedExternalEffects,
                ExternalEffectRequirement::Optional,
                &coordinator,
                &gates,
                &findings,
                &commands,
            ),
            ProductionDecision::Accept,
        );
        assert!(!qualification_ready(
            ProductDeliveryScope::WorkspaceChanges,
            ExternalEffectRequirement::Optional,
            &gates,
            &commands,
        ));
        assert!(qualification_ready(
            ProductDeliveryScope::AuthorizedExternalEffects,
            ExternalEffectRequirement::Optional,
            &gates,
            &commands,
        ));
    }
}
