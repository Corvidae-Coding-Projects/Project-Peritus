//! Exact acceptance-contract to revision-tuple binding tests.

mod support;

use peritus_spec::SpecError;

#[test]
fn binding_requires_the_tuple_to_name_the_exact_contract() {
    let contract = support::contract();
    let revision = support::revision(1, 1);
    let binding = contract.bind(revision).expect("matching specification identity");

    assert_eq!(binding.contract_id(), contract.id());
    assert_eq!(binding.contract_digest(), contract.content_digest());
    assert_eq!(binding.revision(), revision);
    assert!(binding.matches_revision(revision));
    assert!(!binding.matches_revision(support::revision(1, 2)));
    assert_eq!(contract.bind(support::revision(2, 1)), Err(SpecError::RevisionBindingMismatch));
}
