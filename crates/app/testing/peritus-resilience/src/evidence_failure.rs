//! Canonical encoding for qualification and contract failures.

use crate::evidence::{Encoder, fault};
use crate::evidence_tags::{
    corrupt_target, dependency, evidence_kind, failure_phase, recovery_outcome, resource_kind,
};
use crate::{ContractViolation, ScenarioFailure, SubjectError, SubjectErrorCode, SuiteFailure};

pub fn suite_failure(encoder: &mut Encoder, value: &SuiteFailure) {
    match value {
        SuiteFailure::SubjectDescriptorPanic(panic) => {
            encoder.u8(1);
            encoder.u8(failure_phase(panic.phase()));
        }
        SuiteFailure::CatalogExceedsConfiguration { actual, maximum } => {
            encoder.u8(2);
            encoder.usize(*actual);
            encoder.u16(*maximum);
        }
    }
}

pub fn scenario_failure(encoder: &mut Encoder, value: &ScenarioFailure) {
    match value {
        ScenarioFailure::Subject { phase, error } => {
            encoder.u8(1);
            encoder.u8(failure_phase(*phase));
            subject_error(encoder, error);
        }
        ScenarioFailure::Panic(panic) => {
            encoder.u8(2);
            encoder.u8(failure_phase(panic.phase()));
        }
        ScenarioFailure::Contract(contract) => {
            encoder.u8(3);
            violation(encoder, contract);
        }
    }
}

fn subject_error(encoder: &mut Encoder, value: &SubjectError) {
    encoder.u8(match value.code() {
        SubjectErrorCode::Setup => 1,
        SubjectErrorCode::FaultControl => 2,
        SubjectErrorCode::Persistence => 3,
        SubjectErrorCode::Supervision => 4,
        SubjectErrorCode::Recovery => 5,
        SubjectErrorCode::Observation => 6,
        SubjectErrorCode::Cleanup => 7,
        SubjectErrorCode::Unsupported => 8,
    });
    encoder.text(value.context().as_str());
    encoder.bool(value.retryable());
}

fn violation(encoder: &mut Encoder, value: &ContractViolation) {
    match value {
        ContractViolation::ScenarioIdentityMismatch { expected, observed } => {
            encoder.u8(1);
            encoder.text(expected.as_str());
            encoder.text(observed.as_str());
        }
        ContractViolation::FaultIdentityMismatch { expected, observed } => {
            encoder.u8(2);
            fault(encoder, *expected);
            fault(encoder, *observed);
        }
        ContractViolation::BaselineAlreadyAccepted => encoder.u8(3),
        ContractViolation::FaultNotReached => encoder.u8(4),
        ContractViolation::UnexpectedRecovery { expected, observed } => {
            encoder.u8(5);
            encoder.u8(recovery_outcome(*expected));
            encoder.u8(recovery_outcome(*observed));
        }
        ContractViolation::FalseSuccess => encoder.u8(6),
        ContractViolation::ContradictoryAcceptanceEvidence => encoder.u8(7),
        ContractViolation::CrashJournalDivergence => encoder.u8(8),
        ContractViolation::CorruptionNotDetected { expected, observed } => {
            encoder.u8(9);
            encoder.u8(corrupt_target(*expected));
            match observed {
                Some(target) => {
                    encoder.u8(1);
                    encoder.u8(corrupt_target(*target));
                }
                None => encoder.u8(0),
            }
        }
        ContractViolation::UnexpectedCorruption { observed } => {
            encoder.u8(10);
            encoder.u8(corrupt_target(*observed));
        }
        ContractViolation::MutationAdmittedWithCorruption => encoder.u8(11),
        ContractViolation::ProjectionNotRebuilt => encoder.u8(12),
        ContractViolation::ReferencedObjectUnverified => encoder.u8(13),
        ContractViolation::TemporaryObjectLeak { count } => {
            encoder.u8(14);
            encoder.u16(*count);
        }
        ContractViolation::OwnershipScanMissing => encoder.u8(15),
        ContractViolation::OwnershipAccountingInvalid => encoder.u8(16),
        ContractViolation::UnaccountedWork { count } => {
            encoder.u8(17);
            encoder.u16(*count);
        }
        ContractViolation::OrphanedWork { count } => {
            encoder.u8(18);
            encoder.u16(*count);
        }
        ContractViolation::NoOwnedWorkExercised => encoder.u8(19),
        ContractViolation::RetryLimitExceeded { dependency: kind, observed, limit } => {
            encoder.u8(20);
            retry_values(encoder, *kind, *observed, *limit);
        }
        ContractViolation::RetryExhaustionNotReached { dependency: kind, observed, limit } => {
            encoder.u8(21);
            retry_values(encoder, *kind, *observed, *limit);
        }
        ContractViolation::ResourceLimitExceeded { resource, observed, limit } => {
            encoder.u8(22);
            encoder.u8(resource_kind(*resource));
            encoder.u64(*observed);
            encoder.u64(*limit);
        }
        ContractViolation::MissingEvidence(kind) => {
            encoder.u8(23);
            encoder.u8(evidence_kind(*kind));
        }
        ContractViolation::DuplicateEvidence => encoder.u8(24),
        ContractViolation::NonCanonicalMilestones => encoder.u8(25),
        ContractViolation::CleanupIncomplete => encoder.u8(26),
    }
}

fn retry_values(encoder: &mut Encoder, kind: crate::DependencyKind, observed: u16, limit: u16) {
    encoder.u8(dependency(kind));
    encoder.u16(observed);
    encoder.u16(limit);
}
