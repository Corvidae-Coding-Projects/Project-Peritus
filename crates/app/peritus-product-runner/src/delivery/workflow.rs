//! Host-owned selection, stage advancement, and independent evidence admission.

use std::collections::BTreeSet;

use super::{
    Capability, Delivery, DeliveryReview, DraftHandoff, contract::CapabilityKind, invalid,
};
use crate::{
    ProductRunnerError,
    developer_tools::{CommandPurpose, SuccessfulCommand},
};

impl Delivery {
    pub(crate) fn status(&self) -> String {
        if self.planning() {
            return "Peritus: preparing the complete capability plan for independent scope review."
                .into();
        }
        self.active().map_or_else(|| format!(
            "Peritus: all {} capability drafts are present; proceeding with behavioral verification and repair.", self.state.plan.len()),
            |item| format!("Peritus construction: {}; {} of {} capabilities drafted. Drafts are awaiting behavioral verification.",
                item.id, self.state.drafted.len(), self.state.plan.len()))
    }

    pub(crate) fn is_qualified(&self, candidate: &str) -> bool {
        !self.requirements.is_empty()
            && self.requirements.iter().all(|id| self.requirement_evidence(id, candidate).0)
    }

    pub(crate) const fn planning(&self) -> bool {
        !self.state.scope_reviewed || self.state.decomposition.is_some()
    }

    pub(crate) fn constructing(&self) -> bool {
        !self.planning() && self.state.drafted.len() != self.state.plan.len()
    }

    pub(crate) fn active(&self) -> Option<&Capability> {
        let count = self.state.plan.len();
        (0..count)
            .map(|offset| (self.state.cursor + offset) % count)
            .map(|index| &self.state.plan[index])
            .find(|item| {
                !self.state.drafted.contains(&item.id)
                    && item.depends_on.iter().all(|id| self.state.drafted.contains(id))
            })
    }

    pub(crate) fn propose(&mut self, plan: Vec<Capability>) -> Result<(), ProductRunnerError> {
        super::contract::validate_plan(&plan, &self.source, &self.requirements)?;
        if self.state.admitted.iter().any(|old| {
            !plan.iter().any(|new| {
                new.id == old.id
                    && new.outcome == old.outcome
                    && new.requirements == old.requirements
                    && new.positive == old.positive
                    && new.negative == old.negative
                    && new.kind == old.kind
            })
        }) {
            return Err(invalid(
                "an admitted requirement or acceptance criterion cannot be removed or weakened; preserve it and add discovered work",
            ));
        }
        if let Some(oversized) = &self.state.decomposition {
            let parent = plan.iter().find(|item| &item.id == oversized).ok_or_else(|| {
                invalid("decomposition must retain the original required outcome")
            })?;
            if !parent
                .depends_on
                .iter()
                .any(|dependency| !self.state.admitted.iter().any(|old| &old.id == dependency))
            {
                return Err(invalid(
                    "split the oversized capability into smaller prerequisite drafts; keep its original outcome and depend on the new work instead of repeating the same assignment",
                ));
            }
        }
        self.state.plan = plan;
        self.state.drafted.retain(|id| self.state.plan.iter().any(|item| &item.id == id));
        self.state.scope_reviewed = false;
        self.state.qualification = None;
        self.save()
    }

    pub(crate) fn review_scope(
        &mut self,
        review: &DeliveryReview,
        blockers: bool,
    ) -> Result<(), ProductRunnerError> {
        if review.scope_evidence.trim().is_empty() || !review.capabilities.is_empty() {
            return Err(invalid(
                "planning review needs scope evidence and no implementation verdicts",
            ));
        }
        self.state.scope_reviewed = review.scope_complete && !blockers;
        if self.state.scope_reviewed {
            self.state.admitted.clone_from(&self.state.plan);
            if let Some(id) = self.state.decomposition.take() {
                self.state.segments.remove(&id);
            }
        }
        self.state.debt = if self.state.scope_reviewed {
            Vec::new()
        } else {
            vec![review.scope_evidence.clone()]
        };
        self.save()
    }

    pub(crate) fn draft(&mut self, handoff: DraftHandoff) -> Result<(), ProductRunnerError> {
        if !self.constructing()
            || self.active().is_none_or(|item| item.id != handoff.capability)
            || handoff.implementation.trim().is_empty()
        {
            return Err(invalid(
                "construction handoff must name the host-selected capability and its concrete implementation",
            ));
        }
        let index = self
            .state
            .plan
            .iter()
            .position(|item| item.id == handoff.capability)
            .ok_or_else(|| invalid("construction capability disappeared"))?;
        if handoff.remaining.is_empty() {
            self.state.debt.retain(|debt| !debt.starts_with(&format!("{}:", handoff.capability)));
            self.state.draft_evidence.insert(handoff.capability.clone(), handoff.implementation);
            self.state.segments.remove(&handoff.capability);
            self.state.drafted.insert(handoff.capability);
        } else {
            self.state.debt.extend(
                handoff.remaining.into_iter().map(|debt| format!("{}: {debt}", handoff.capability)),
            );
            self.state.debt.sort();
            self.state.debt.dedup();
            return self.segment_boundary();
        }
        self.state.cursor = (index + 1) % self.state.plan.len();
        self.state.qualification = None;
        self.save()
    }

    /// A segment boundary changes selection, never changes completion status or stops the run.
    pub(crate) fn rotate(&mut self) -> Result<(), ProductRunnerError> {
        if let Some(active) = self.active() {
            let index = self.state.plan.iter().position(|item| item.id == active.id).unwrap_or(0);
            self.state.cursor = (index + 1) % self.state.plan.len();
        }
        self.state.qualification = None;
        self.save()
    }

    /// Repeated uncompleted segments require smaller work. This replans rather than ending
    /// the run: no execution budget is exhausted and existing implementations remain intact.
    pub(crate) fn segment_boundary(&mut self) -> Result<(), ProductRunnerError> {
        if self.constructing()
            && let Some(id) = self.active().map(|item| item.id.clone())
        {
            let segments = self.state.segments.entry(id.clone()).or_default();
            *segments = segments.saturating_add(1);
            if *segments >= 3 {
                self.state.decomposition = Some(id.clone());
                self.state.debt.push(format!("{id}: construction segments ended without a draft handoff. Split the remaining work into smaller prerequisite drafts; preserve the original outcome and existing progress."));
            }
        }
        self.rotate()
    }

    pub(crate) fn prompt(&self) -> Result<String, ProductRunnerError> {
        let plan =
            serde_json::to_string(&self.state.plan).map_err(|error| invalid(error.to_string()))?;
        let stage = if self.planning() {
            "PLANNING: read-only scope decomposition"
        } else if self.constructing() {
            "CONSTRUCTION: first implementation across the entire scope"
        } else {
            "VERIFICATION AND REPAIR: exercise the complete draft, then refine"
        };
        Ok(format!(
            "\n\nHOST-OWNED DELIVERY STAGE: {stage}\nRequired obligation IDs: {:?}\nAdmitted capabilities: {plan}\nDrafted (not verified): {:?}\nSelected construction capability: {:?}\nOpen implementation debt: {:?}\nPrior scope contract retained after amendment: {:?}",
            self.requirements,
            self.state.drafted,
            self.active().map(|item| &item.id),
            self.state.debt,
            self.previous()
        ))
    }

    pub(crate) fn review_prompt(&self, scope_only: bool) -> Result<String, ProductRunnerError> {
        Ok(format!(
            "{}\n\nINDEPENDENT CAPABILITY REVIEW. {} Append `delivery` to the normal review JSON: {{\"scope_complete\":true,\"scope_evidence\":\"trace the original request and accepted design to the complete capability plan, including implicit obligations\",\"capabilities\":[{{\"id\":\"capability ID\",\"satisfied\":false,\"positive_observation\":\"observed result or exact missing behavior\",\"negative_observation\":\"failure/rejection/preservation/recovery result, or why not applicable\",\"commands\":[\"host-issued verification reference from the available list\"]}}]}}. Missing implementation, blanket unsupported responses, no-op success, unreachable handlers, mocks, and disabled tests cannot establish requested executable behavior. A legitimate protocol rejection is evidence only for its rejection requirement, never a replacement for the required successful operation. Treat behavior mislabeled as an artifact as a scope defect. Inspect the real implementation and callers. Do not accept a report or a global green suite as evidence for every capability. Mark unproved capabilities unsatisfied and explain the repair; inability to verify is not completion.",
            self.prompt()?,
            if scope_only {
                "This is a PLAN review before construction. Check decomposition against original user scope, preserve working behavior, reject omissions or optional polish disguised as a prerequisite. Return an empty capabilities array; do not require implementations or test results at this stage."
            } else {
                "This is a FINAL candidate review. Return exactly one verdict for every capability. For behavior, cite short host-issued verification references for commands that actually exercise that capability's positive and applicable negative criteria on this candidate. For artifacts, independently read back the actual requested artifact. A relevant real verification process may cover several capabilities. Inspect its assertions, actual interface, decisive results and exit status; do not repeatedly reconstruct every successful generated row after coverage is established. Use developer_evidence_read for exact current evidence rather than searching memory for old recovery receipts. Scope completeness and every capability's evidence are required separately."
            }
        ))
    }

    pub(crate) fn check_review(
        &self,
        review: &DeliveryReview,
        scope_only: bool,
        candidate: &str,
        commands: &[SuccessfulCommand],
    ) -> Result<bool, ProductRunnerError> {
        if scope_only {
            if review.scope_evidence.trim().is_empty() || !review.capabilities.is_empty() {
                return Err(invalid(
                    "scope review requires coverage evidence and no implementation verdicts",
                ));
            }
            return Ok(review.scope_complete);
        }
        if self.planning() || self.constructing() {
            return Err(invalid("unfinished construction cannot enter final qualification"));
        }
        let mut seen = BTreeSet::new();
        for assessment in &review.capabilities {
            let item = self
                .state
                .plan
                .iter()
                .find(|item| item.id == assessment.id)
                .ok_or_else(|| invalid("review names an unknown capability"))?;
            if !seen.insert(&assessment.id)
                || assessment.positive_observation.trim().is_empty()
                || assessment.negative_observation.trim().is_empty()
            {
                return Err(invalid(
                    "capability review must be unique and include positive and failure observations",
                ));
            }
            if assessment.satisfied
                && item.kind == CapabilityKind::Behavior
                && (assessment.commands.is_empty()
                    || assessment.commands.iter().any(|reference| {
                        !commands.iter().any(|command| {
                            command.verification_reference() == *reference
                                && command.purpose == CommandPurpose::Verification
                                && command.observed_candidate.as_deref() == Some(candidate)
                        })
                    }))
            {
                return Err(invalid(format!(
                    "{} has no matching successful verification on the current candidate",
                    item.id
                )));
            }
        }
        if seen.len() != self.state.plan.len() || review.scope_evidence.trim().is_empty() {
            return Err(invalid("independent review omitted capability or scope coverage"));
        }
        Ok(review.scope_complete && review.capabilities.iter().all(|item| item.satisfied))
    }

    pub(crate) fn require_verification_handoff(
        &self,
        candidate: &str,
        commands: &[SuccessfulCommand],
    ) -> Result<(), ProductRunnerError> {
        if self.state.plan.iter().any(|item| item.kind == CapabilityKind::Behavior)
            && !commands.iter().any(|command| {
                command.purpose == CommandPurpose::Verification
                    && command.observed_candidate.as_deref() == Some(candidate)
            })
        {
            return Err(invalid(
                "No successful behavior verification is bound to the final candidate. Finish source, documentation and workspace cleanup first, then run fresh real-interface verification. Place disposable drivers outside the delivered candidate or clean them up before the final check. Historical results remain available but cannot qualify changed files. Respect command restrictions and report any inability to verify instead of claiming completion.",
            ));
        }
        Ok(())
    }

    pub(crate) fn qualify(
        &mut self,
        review: DeliveryReview,
        candidate: &str,
        commands: &[SuccessfulCommand],
    ) -> Result<bool, ProductRunnerError> {
        let qualified = self.check_review(&review, false, candidate, commands)?;
        self.state.debt = review
            .capabilities
            .iter()
            .filter(|item| !item.satisfied)
            .map(|item| {
                format!("{}: {}; {}", item.id, item.positive_observation, item.negative_observation)
            })
            .collect();
        if !review.scope_complete {
            self.state.debt.push(review.scope_evidence.clone());
        }
        self.state.qualification = Some((candidate.to_owned(), review));
        self.state.qualification_revision = Some(self.revision);
        self.save()?;
        Ok(qualified)
    }

    pub(crate) fn requirement_evidence(
        &self,
        requirement: &str,
        candidate: &str,
    ) -> (bool, String) {
        let Some((observed, review)) = &self.state.qualification else {
            return (false, "missing capability review".into());
        };
        if self.state.qualification_revision != Some(self.revision)
            || observed != candidate
            || !review.scope_complete
            || self.constructing()
            || self.planning()
        {
            return (false, "stale or incomplete capability review".into());
        }
        let mapped: Vec<_> = self
            .state
            .plan
            .iter()
            .filter(|item| item.requirements.iter().any(|id| id == requirement))
            .map(|item| {
                let mut matches =
                    review.capabilities.iter().filter(|assessment| assessment.id == item.id);
                let assessment = matches.next();
                if matches.next().is_some() { None } else { assessment }
            })
            .collect();
        let satisfied = !mapped.is_empty()
            && mapped.iter().all(|item| {
                item.is_some_and(|proof| {
                    proof.satisfied
                        && !proof.positive_observation.trim().is_empty()
                        && !proof.negative_observation.trim().is_empty()
                })
            });
        (satisfied, format!("{mapped:?}"))
    }
}

pub const DEVELOPER_POLICY: &str = r#"The user requested the complete capability, including applicable domain obligations. Never silently narrow it, replace required behavior with unsupported/not implemented, delete tests, or relabel unfinished code as complete. Intermediate scaffolding is allowed but remains open debt. Renaming a stub does not discharge it.
During PLANNING, return {"kind":"plan","capabilities":[{"id":"stable-id","requirements":["host requirement ID"],"source_quote":"exact substring of the user request or accepted design","outcome":"required observable capability","depends_on":[],"kind":"behavior","positive":"real success criterion","negative":"required failure/preservation/recovery criterion or applicability rationale"}]}. Artifact-only work uses kind artifact. Cover every requirement and implicit domain obligation. Keep dependency foundations minimal; do not disguise optional polish as a dependency. This is read-only and precedes independent scope review.
During CONSTRUCTION, implement the selected capability's substantive first draft and its required callers. Favor complete breadth in dependency order. Use cheap compile/startup checks or a targeted check needed to unblock construction. Defer broad suites, exhaustive edge-case testing, refactoring, hardening, and optimization until draft coverage. Never confuse a rough draft with verified behavior. Return {"kind":"draft","handoff":{"capability":"selected ID","implementation":"concrete implementation paths, connected behavior, and what now exists","remaining":[]}}. If blocked or incomplete, put the exact missing implementation and evidence in remaining; it stays open while other ready work proceeds. Reopen drafted code during construction only for a concrete build failure, downstream dependency blocker, critical observed defect, or current user steering, with that reason in the handoff. Scope amendments may add required capabilities with a plan terminal; removing admitted outcomes requires an actual user scope change. A segment boundary reassesses the next missing capability; it is not permission to polish the current area indefinitely.
During VERIFICATION AND REPAIR, operate the actual complete system through relevant interfaces, verify success and applicable negative/recovery cases, then repair concrete failures and refine. Respect user command restrictions; report unverifiable guarantees rather than claiming them. A complete terminal only hands the whole candidate to independent gates and capability review. It never self-certifies functional completion. User questions and steering take priority in every stage. No token, elapsed-time, or task budget is introduced by this scheduler."#;
