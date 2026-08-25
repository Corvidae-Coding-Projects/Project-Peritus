//! Pure causal trace projection over checked C0 records.

mod encoding;
mod fold;
mod state;

use std::num::NonZeroU64;

use peritus_projection::{
    FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionIdentity,
    ProjectionName, ProjectionSchema, ProjectionVersion,
};

pub use state::{ProjectedObservation, SpanSnapshot, TraceProjectionState, TraceSnapshot};

use crate::{TRACE_OBSERVATION_FAMILY, TraceError, TraceErrorKind};

/// Version-one pure trace projection for C0 replay and shadow generations.
#[derive(Clone, Debug)]
pub struct TraceProjection {
    schema: ProjectionSchema,
}

impl TraceProjection {
    /// Creates the immutable version-one projection schema.
    ///
    /// # Errors
    ///
    /// Returns a projection identity error only if built-in constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        let name = ProjectionName::new("causal-traces")?;
        let identity = ProjectionIdentity::new(name, ProjectionVersion::new(NonZeroU64::MIN));
        ProjectionSchema::new(
            identity,
            b"trace-observation:60:v1;causal-parent;span-lifecycle;redaction;vault;otel",
        )
        .map(|schema| Self { schema })
    }
}

impl Projection for TraceProjection {
    type State = TraceProjectionState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        TraceProjectionState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        if input.family() != TRACE_OBSERVATION_FAMILY {
            return Ok(());
        }
        state.apply_record(input.record()).map(|_| ()).map_err(|error| trace_to_projection(&error))
    }
}

fn trace_to_projection(error: &TraceError) -> ProjectionError {
    let kind = match error.kind() {
        TraceErrorKind::InvalidFrame => ProjectionErrorKind::InvalidFrame,
        _ => ProjectionErrorKind::FoldInvariant,
    };
    projection_error(kind, "trace observation violates causal projection invariants")
}

fn projection_error(kind: ProjectionErrorKind, detail: &'static str) -> ProjectionError {
    ProjectionError::fold(
        kind,
        peritus_projection::RecoveryClass::RepairJournal,
        "fold causal trace",
        detail,
    )
}
