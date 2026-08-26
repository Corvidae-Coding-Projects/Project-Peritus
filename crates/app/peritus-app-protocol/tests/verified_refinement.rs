//! Executable refinement checks paired with A3 Verus specifications.

use peritus_app_protocol::verified::{
    ack_legal, chunk_accepted, delivery_advances, negotiation_safe, redelivery_identity,
    terminal_exit, terminal_output, transfer_complete, version_supported, within_bound,
};

#[test]
fn negotiation_predicates_cover_success_and_failure_boundaries() {
    assert!(version_supported(1, 4, 1, 2, 5));
    assert!(!version_supported(2, 4, 1, 2, 5));
    assert!(!version_supported(1, 1, 1, 2, 5));
    assert!(negotiation_safe(true, true, true, true));
    assert!(!negotiation_safe(true, false, true, true));
}

#[test]
fn delivery_and_ack_predicates_enforce_contiguous_monotonic_progress() {
    assert!(delivery_advances(0, 1));
    assert!(!delivery_advances(1, 3));
    assert!(ack_legal(2, 5, 4, false));
    assert!(!ack_legal(4, 5, 3, false));
    assert!(!ack_legal(2, 5, 6, false));
    assert!(!ack_legal(2, 5, 4, true));
    assert!(redelivery_identity(true, true, true, true, true));
    assert!(!redelivery_identity(true, true, false, true, true));
}

#[test]
fn transfer_predicates_conserve_exact_size() {
    assert!(chunk_accepted(4, 4, 3, 10, 4));
    assert!(!chunk_accepted(4, 5, 3, 10, 4));
    assert!(!chunk_accepted(8, 8, 3, 10, 4));
    assert!(transfer_complete(10, 10, true, false));
    assert!(!transfer_complete(9, 10, true, false));
    assert!(!transfer_complete(10, 10, true, true));
}

#[test]
fn terminal_and_bound_predicates_reject_post_terminal_or_excess_input() {
    assert!(terminal_output(2, 3, 10, 10, false));
    assert!(!terminal_output(2, 3, 10, 10, true));
    assert!(terminal_exit(3, 4, false));
    assert!(!terminal_exit(3, 4, true));
    assert!(within_bound(4, 4));
    assert!(!within_bound(5, 4));
    assert!(!within_bound(0, 0));
}
