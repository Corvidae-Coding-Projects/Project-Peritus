//! Boundary tests for all one-based number types.

use peritus_types::{EventSequence, Generation, OneBasedNumberError, RevisionNumber};

fn assert_one_based<T: Copy + std::fmt::Debug + Eq>(
    first: fn() -> T,
    new: fn(u64) -> Result<T, OneBasedNumberError>,
    get: fn(T) -> u64,
    next: fn(T) -> Result<T, OneBasedNumberError>,
) {
    assert_eq!(new(0), Err(OneBasedNumberError::Zero));
    assert_eq!(get(first()), 1);
    let current = new(41).expect("positive number");
    assert_eq!(get(next(current).expect("increment")), 42);
    assert_eq!(
        next(new(u64::MAX).expect("maximum remains positive")),
        Err(OneBasedNumberError::Overflow)
    );
}

#[test]
fn revision_numbers_are_one_based_and_checked() {
    assert_one_based(
        RevisionNumber::first,
        RevisionNumber::new,
        RevisionNumber::get,
        RevisionNumber::checked_next,
    );
}

#[test]
fn event_sequences_are_one_based_and_checked() {
    assert_one_based(
        EventSequence::first,
        EventSequence::new,
        EventSequence::get,
        EventSequence::checked_next,
    );
}

#[test]
fn generations_are_one_based_and_checked() {
    assert_one_based(Generation::first, Generation::new, Generation::get, Generation::checked_next);
}
