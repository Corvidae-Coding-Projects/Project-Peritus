//! Durable deterministic review cycles and finding lifecycle for Peritus.

mod binding;
mod canonical;
mod command;
mod disposition;
mod durability;
mod error;
mod event;
mod finding;
mod history;
mod limits;
mod observation;
mod oscillation;
#[cfg(not(verus_only))]
mod product;
mod projection;
mod quorum;
mod reconciliation;
mod reducer;
mod replay;
mod reviewer;
mod state;
mod verified;
mod waiver;
mod wire;

pub use binding::{ReviewBinding, ReviewBindingVersion};
pub use command::{ReviewCommand, ReviewCommandKind};
pub use disposition::{DispositionKind, DispositionRecord, FixerResponse};
pub use durability::{
    REVIEW_STATE_NAMESPACE, commit_review_command, commit_review_transition,
    load_review_checkpoint, load_review_event, load_review_replay,
};
pub use error::{ReviewError, ReviewErrorKind, ReviewRecoveryAction};
pub use event::{ReviewEvent, ReviewEventKind, ReviewTransition};
pub use finding::{Finding, FindingLocation, FindingSource, ReviewSubmission};
pub use limits::{Confidence, ReviewLimits};
pub use observation::QualityProjection;
pub use oscillation::{OscillationKind, OscillationReport};
#[cfg(not(verus_only))]
pub use product::{
    PRODUCT_FINDING_SOURCE_ORDINAL_BASE, FindingSeverity, ProductFinding, ProductFindingBody,
    ProductFindingBodyFields, ProductFindingBodyPublisher, ProductFindingBodyReference,
    ProductFindingCategory, ProductFindingFieldReference, ProductFindingLedger,
    ProductFindingPreview, ProductFindingState, ProductReviewError, ProductReviewPage,
    ProductReviewSubmission, ProductReviewSummaryReference,
};
pub use projection::{ProjectedCycle, ProjectedFinding, ReviewProjection};
pub use quorum::{QuorumDimension, QuorumReport};
pub use reducer::{decide, replay, start};
pub use replay::ReviewReplay;
pub use reviewer::{ReviewAssignment, ReviewCycle, ReviewCyclePhase};
pub use state::{
    ReviewHistoryFrontier, ReviewRunPhase, ReviewRunState, ReviewTerminal, ReviewTerminalKind,
};
pub use verified::{
    evidence_is_fresh, findings_are_conserved, no_implicit_success, quorum_is_complete,
    replay_equivalent, transition_is_legal,
};
pub use waiver::ObservedWaiver;
pub use wire::{ReviewCommandFrame, ReviewEventFrame};
