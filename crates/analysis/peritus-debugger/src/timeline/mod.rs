//! Deterministic per-subject causal timelines.

mod builder;

pub use builder::{
    BoundaryKind, ClockAmbiguity, ResourceObservation, Timeline, TimelineEntry, build_timelines,
    build_timelines_controlled,
};
