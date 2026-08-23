//! Exact participant-set projections used by reducer contracts.

#[cfg(verus_only)]
use peritus_types::ActorId;
use vstd::prelude::*;

verus! {

pub(super) open spec fn contains(values: Seq<ActorId>, actor: ActorId) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] crate::state::exact::same_identifier_from(
            values[index].spec_bytes(),
            actor.spec_bytes(),
            0,
        )
}

} // verus!
