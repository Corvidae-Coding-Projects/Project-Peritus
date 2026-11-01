//! Public-API regression for future quarantine generations on rooted inventory entries.
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReferenceSet, CollectionGeneration, ErrorCode, GcAction,
    GcInventoryEntry, GcPlan, QuarantineState, ReferenceRoots,
};

fn plan(generation: u64, since: u64, marked: bool) -> Result<GcPlan, String> {
    let digest = ArtifactDigest::new([7; 32]);
    let mut roots = ArtifactReferenceSet::new();
    if marked {
        roots.insert(digest);
    }
    GcPlan::build(
        CollectionGeneration::new(generation).map_err(|e| e.to_string())?,
        [GcInventoryEntry::new(
            digest,
            5,
            QuarantineState::Quarantined {
                since: CollectionGeneration::new(since).map_err(|e| e.to_string())?,
            },
        )],
        &ReferenceRoots::new(roots, ArtifactReferenceSet::new()),
    )
    .map_err(|e| {
        if e.code() == ErrorCode::InvalidCollectionPlan {
            "invalid collection plan".into()
        } else {
            e.to_string()
        }
    })
}
fn main() -> Result<(), String> {
    for marked in [false, true] {
        match plan(1, 2, marked) {
            Err(error) if error == "invalid collection plan" => {}
            other => {
                return Err(format!(
                    "future quarantine must reject for marked={marked}; observed {other:?}"
                ));
            }
        }
    }
    for (generation, since) in [(1, 1), (2, 1)] {
        let plan = plan(generation, since, true)?;
        if !matches!(plan.actions(),[GcAction::Restore{since:actual,..}] if actual.get()==since) {
            return Err("valid rooted quarantine no longer restores".into());
        }
    }
    println!(
        "Future quarantines reject independently of roots; current and older rooted quarantines still restore."
    );
    Ok(())
}
