//! Printable names and allocation-bounded integer/hex encodings.

use peritus_policy::{ActorRole, AuthorityTier, IndependenceRequirement, PolicyTier, RiskClass};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub(super) fn hex(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len()
        invariant 0 <= index <= bytes.len(),
        decreases bytes.len() - index,
    {
        output.push(hex_digit(bytes[index] >> 4));
        output.push(hex_digit(bytes[index] & 0x0f));
        index += 1;
    }
    output
}

const fn hex_digit(value: u8) -> char {
    match value {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'a',
        11 => 'b',
        12 => 'c',
        13 => 'd',
        14 => 'e',
        _ => 'f',
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "value modulo ten is always representable as one decimal u8 digit"
)]
pub(super) fn decimal_usize(mut value: usize) -> String {
    let mut reversed = Vec::new();
    if value == 0 {
        reversed.push(b'0');
    } else {
        while value > 0
            decreases value,
        {
            reversed.push(b'0' + (value % 10) as u8);
            value /= 10;
        }
    }
    let mut output = String::new();
    let mut index = reversed.len();
    while index > 0
        invariant 0 <= index <= reversed.len(),
        decreases index,
    {
        index -= 1;
        output.push(reversed[index] as char);
    }
    output
}

pub(super) fn decimal_u64(mut value: u64) -> String {
    let mut reversed = Vec::new();
    if value == 0 {
        reversed.push(b'0');
    } else {
        while value > 0
            decreases value,
        {
            reversed.push(b'0' + (value % 10) as u8);
            value /= 10;
        }
    }
    let mut output = String::new();
    let mut index = reversed.len();
    while index > 0
        invariant 0 <= index <= reversed.len(),
        decreases index,
    {
        index -= 1;
        output.push(reversed[index] as char);
    }
    output
}

pub(super) const fn role_name(value: ActorRole) -> &'static str {
    match value {
        ActorRole::Writer => "writer",
        ActorRole::Fixer => "fixer",
        ActorRole::Reviewer => "reviewer",
        ActorRole::Evaluator => "evaluator",
        ActorRole::GateRunner => "gate-runner",
        ActorRole::Orchestrator => "orchestrator",
        ActorRole::EvolutionAgent => "evolution-agent",
        ActorRole::HumanAuthority => "human-authority",
        ActorRole::DaemonService => "daemon-service",
        ActorRole::ProviderToolWorker => "provider-tool-worker",
        ActorRole::Plugin => "plugin",
    }
}

pub(super) const fn risk_name(value: RiskClass) -> &'static str {
    match value {
        RiskClass::Read => "read",
        RiskClass::ScopedWrite => "scoped-write",
        RiskClass::Execution => "execution",
        RiskClass::Network => "network",
        RiskClass::DependencyEnvironment => "dependency-environment",
        RiskClass::RepositoryHistoryMutation => "repository-history-mutation",
        RiskClass::SecretUse => "secret-use",
        RiskClass::ExternalSideEffect => "external-side-effect",
        RiskClass::PolicyAuthority => "policy-authority",
        RiskClass::HarnessPromotion => "harness-promotion",
    }
}

pub(super) const fn authority_tier_name(value: AuthorityTier) -> &'static str {
    match value {
        AuthorityTier::Project => "project",
        AuthorityTier::User => "user",
        AuthorityTier::Organization => "organization",
        AuthorityTier::System => "system",
    }
}

pub(super) const fn independence_name(value: IndependenceRequirement) -> &'static str {
    match value {
        IndependenceRequirement::NotRequester => "not-requester",
        IndependenceRequirement::NotActionActor => "not-action-actor",
        IndependenceRequirement::NoProducingAttemptParticipation => {
            "no-producing-attempt-participation"
        }
        IndependenceRequirement::NoReviewParticipation => "no-review-participation",
    }
}

pub(super) const fn choice_name(value: crate::ApprovalChoice) -> &'static str {
    match value {
        crate::ApprovalChoice::Deny => "deny",
        crate::ApprovalChoice::ApproveOnce => "approve-once",
        crate::ApprovalChoice::Amend(_) => "amend",
    }
}

pub(super) const fn policy_tier_name(value: PolicyTier) -> &'static str {
    match value {
        PolicyTier::System => "system",
        PolicyTier::User => "user",
        PolicyTier::Project => "project",
        PolicyTier::Run => "run",
        PolicyTier::Session => "session",
        PolicyTier::RoleHarness => "role-harness",
    }
}

pub(super) const fn phase_name(value: crate::ApprovalPhase) -> &'static str {
    match value {
        crate::ApprovalPhase::Pending => "pending",
        crate::ApprovalPhase::ApprovedOnce => "approved-once",
        crate::ApprovalPhase::AmendmentAuthorized => "amendment-authorized",
        crate::ApprovalPhase::Consumed => "consumed",
        crate::ApprovalPhase::Amended => "amended",
        crate::ApprovalPhase::Denied => "denied",
        crate::ApprovalPhase::Expired => "expired",
        crate::ApprovalPhase::Cancelled => "cancelled",
    }
}

pub(super) fn digest_value(value: Sha256Digest) -> String { hex(value.as_bytes()) }

} // verus!
