//! Independent scalar policy model used by generated and exhaustive tests.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelWindow {
    pub start: u64,
    pub end: u64,
}

impl ModelWindow {
    pub const fn intersect(self, other: Self) -> Option<Self> {
        let start = if self.start >= other.start { self.start } else { other.start };
        let end = if self.end <= other.end { self.end } else { other.end };
        if start < end { Some(Self { start, end }) } else { None }
    }

    pub const fn contains(self, tick: u64) -> bool {
        self.start <= tick && tick < self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelApproval {
    pub minimum_tier: u8,
    pub role_bits: u16,
    pub independence_bits: u8,
    pub validity: ModelWindow,
}

impl ModelApproval {
    pub const fn conjunction(self, other: Self) -> Option<Self> {
        let role_bits = self.role_bits & other.role_bits;
        let Some(validity) = self.validity.intersect(other.validity) else {
            return None;
        };
        if role_bits == 0 {
            return None;
        }
        Some(Self {
            minimum_tier: if self.minimum_tier >= other.minimum_tier {
                self.minimum_tier
            } else {
                other.minimum_tier
            },
            role_bits,
            independence_bits: self.independence_bits | other.independence_bits,
            validity,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRule {
    Neutral,
    Deny,
    Approval(ModelApproval),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelGrant {
    pub covers: Vec<bool>,
    pub applicable: bool,
    pub validity: ModelWindow,
    pub uses: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequest {
    pub pair_count: usize,
    pub validity: ModelWindow,
    pub uses: Option<u64>,
    pub observed_tick: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelGate {
    Satisfied,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelInput {
    pub policy: ModelGate,
    pub boundary: ModelGate,
    pub operation: ModelGate,
    pub immutable: ModelGate,
    pub boundary_validity: ModelWindow,
    pub boundary_uses: Option<u64>,
    pub request: ModelRequest,
    pub grants: Vec<ModelGrant>,
    pub rules: Vec<ModelRule>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelDenial {
    PolicyMismatch,
    OutsideBoundary,
    Operation,
    Immutable,
    Restriction,
    IncompleteCoverage,
    ConstraintConflict,
    NotYetValid,
    Expired,
    ApprovalConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelDecision {
    Authorized { validity: ModelWindow, uses: Option<u64> },
    ApprovalRequired { validity: ModelWindow, uses: Option<u64>, requirement: ModelApproval },
    Denied(ModelDenial),
}

pub fn minimum_uses(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) => Some(left.min(right)),
    }
}

/// Evaluates a policy without calling or sharing code with `peritus-policy`.
pub fn evaluate_model(input: &ModelInput) -> ModelDecision {
    if input.policy == ModelGate::Rejected {
        return ModelDecision::Denied(ModelDenial::PolicyMismatch);
    }
    if input.boundary == ModelGate::Rejected {
        return ModelDecision::Denied(ModelDenial::OutsideBoundary);
    }
    if input.operation == ModelGate::Rejected {
        return ModelDecision::Denied(ModelDenial::Operation);
    }
    if input.immutable == ModelGate::Rejected {
        return ModelDecision::Denied(ModelDenial::Immutable);
    }
    if input.rules.iter().any(|rule| matches!(rule, ModelRule::Deny)) {
        return ModelDecision::Denied(ModelDenial::Restriction);
    }
    for pair in 0..input.request.pair_count {
        if !input
            .grants
            .iter()
            .any(|grant| grant.applicable && grant.covers.get(pair) == Some(&true))
        {
            return ModelDecision::Denied(ModelDenial::IncompleteCoverage);
        }
    }

    let Some(mut validity) = input.request.validity.intersect(input.boundary_validity) else {
        return ModelDecision::Denied(ModelDenial::ConstraintConflict);
    };
    let mut uses = minimum_uses(input.request.uses, input.boundary_uses);
    for grant in input.grants.iter().filter(|grant| grant.applicable) {
        let Some(next) = validity.intersect(grant.validity) else {
            return ModelDecision::Denied(ModelDenial::ConstraintConflict);
        };
        validity = next;
        uses = minimum_uses(uses, grant.uses);
    }
    if !validity.contains(input.request.observed_tick) {
        return if input.request.observed_tick < validity.start {
            ModelDecision::Denied(ModelDenial::NotYetValid)
        } else {
            ModelDecision::Denied(ModelDenial::Expired)
        };
    }

    let mut approval = None;
    for requirement in input.rules.iter().filter_map(|rule| match rule {
        ModelRule::Approval(requirement) => Some(*requirement),
        ModelRule::Neutral | ModelRule::Deny => None,
    }) {
        approval = match approval {
            None => Some(requirement),
            Some(accumulated) => match accumulated.conjunction(requirement) {
                Some(value) => Some(value),
                None => return ModelDecision::Denied(ModelDenial::ApprovalConflict),
            },
        };
    }
    match approval {
        None => ModelDecision::Authorized { validity, uses },
        Some(requirement) => {
            let Some(approval_validity) = requirement.validity.intersect(validity) else {
                return ModelDecision::Denied(ModelDenial::ApprovalConflict);
            };
            ModelDecision::ApprovalRequired {
                validity,
                uses,
                requirement: ModelApproval { validity: approval_validity, ..requirement },
            }
        }
    }
}
