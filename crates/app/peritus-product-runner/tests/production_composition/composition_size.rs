//! Stack-size guard for callers that compose the product-run future.

use super::*;

#[test]
fn product_run_future_leaves_room_for_composing_callers() {
    const fn future_size<A, B, F: Future>(_: fn(A, B) -> F) -> usize {
        size_of::<F>()
    }
    let bytes = future_size(ProductRunner::run);
    assert!(bytes <= 8192, "product run future uses {bytes} bytes before caller state");
}
