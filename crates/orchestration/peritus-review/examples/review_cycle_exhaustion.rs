//! Deterministic public-API replay for exhaustion of the review-cycle counter.
//!
//! Run with `cargo run --locked -p peritus-review --example review_cycle_exhaustion`.

use peritus_review::{
    FindingSeverity, ProductFinding, ProductFindingCategory, ProductFindingLedger,
    ProductReviewSubmission,
};

fn main() -> Result<(), String> {
    let finding = ProductFinding::new(
        ProductFindingCategory::Correctness,
        FindingSeverity::High,
        "Retain the outstanding finding".to_owned(),
        "An untouched finding remains open across a reviewer omission.".to_owned(),
        "source.rs:7".to_owned(),
        String::new(),
        "Complete the fix and obtain a fresh review.".to_owned(),
        u32::MAX - 1,
    )
    .map_err(|error| error.to_string())?;
    let mut ledger =
        ProductFindingLedger::restore(u32::MAX - 1, "Penultimate review".to_owned(), vec![finding])
            .map_err(|error| error.to_string())?;

    let last_review = ProductReviewSubmission::new("Final representable review".to_owned(), vec![])
        .map_err(|error| error.to_string())?;
    ledger.admit_review(u32::MAX, last_review).map_err(|error| error.to_string())?;
    if ledger.cycle() != u32::MAX || !ledger.has_blockers() {
        return Err("the final valid review lost its cycle or untouched finding".to_owned());
    }

    let before = ledger.clone();
    let repeated =
        ProductReviewSubmission::new("This must not replace the head".to_owned(), vec![])
            .map_err(|error| error.to_string())?;
    let error = match ledger.admit_review(u32::MAX, repeated) {
        Ok(()) => return Err("a review advanced beyond the final representable cycle".to_owned()),
        Err(error) => error,
    };
    if error.detail() != "review cycle is stale or nonmonotonic" || ledger != before {
        return Err(
            "exhaustion must reject without changing summary, findings, or cycle".to_owned()
        );
    }
    println!("Final valid review accepted; repeated maximum cycle rejected without state changes.");
    Ok(())
}
