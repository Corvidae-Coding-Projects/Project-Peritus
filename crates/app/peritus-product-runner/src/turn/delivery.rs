//! Stage handoffs happen between invocations; they never certify task completion.

use crate::{
    ProductRunnerError, budget::RunAccounting, delivery::Delivery, execution::ProductRunInput,
    reviewer_turn,
};

use super::terminal::TerminalTurn;
use core::fmt::Write as _;

pub(super) async fn advance(
    input: &ProductRunInput,
    delivery: &mut Delivery,
    design: &crate::design::DesignDocument,
    terminal: TerminalTurn,
    accounting: &mut RunAccounting,
    commands: &[crate::developer_tools::SuccessfulCommand],
) -> Result<Option<TerminalTurn>, ProductRunnerError> {
    match terminal {
        TerminalTurn::Plan(plan) => {
            delivery.propose(plan)?;
            let revision = input.conversation.revision();
            let transcript = input.conversation.render();
            let submission = reviewer_turn::complete(
                input,
                0,
                1,
                reviewer_turn::ReviewEvidence {
                    conversation: &transcript,
                    design,
                    diff: "",
                    gates: "Scope review before construction; no implementation is being accepted.",
                    developer_commands: "",
                    prior: "",
                    delivery: Some(reviewer_turn::DeliveryReviewContext {
                        ledger: delivery,
                        scope_only: true,
                        candidate: "",
                        commands: &[],
                    }),
                },
                accounting,
            )
            .await?;
            if input.conversation.revision() != revision {
                return Ok(None);
            }
            let mut assessment = submission
                .delivery
                .ok_or_else(|| crate::delivery::invalid("independent scope review is missing"))?;
            let blockers = submission
                .submission
                .findings()
                .iter()
                .any(peritus_review::ProductFinding::blocking);
            for finding in
                submission.submission.findings().iter().filter(|finding| finding.blocking())
            {
                let _ = write!(
                    assessment.scope_evidence,
                    "\n{}: {}. Required repair: {}",
                    finding.title(),
                    finding.description(),
                    finding.remediation()
                );
            }
            delivery.review_scope(&assessment, blockers)?;
            Ok(None)
        }
        TerminalTurn::Draft(handoff) => {
            delivery.draft(handoff)?;
            Ok(None)
        }
        TerminalTurn::Complete(_) if delivery.planning() || delivery.constructing() => {
            Err(crate::delivery::invalid(
                "whole-task completion is unavailable while required capabilities lack an independently reviewed plan or construction handoff",
            ))
        }
        TerminalTurn::Complete(summary) => {
            delivery.require_verification_handoff(&crate::delivery::candidate(input)?, commands)?;
            Ok(Some(TerminalTurn::Complete(summary)))
        }
        terminal @ TerminalTurn::Question(_) => Ok(Some(terminal)),
    }
}
