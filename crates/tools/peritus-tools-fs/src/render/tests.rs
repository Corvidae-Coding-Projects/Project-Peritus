use super::{array, object, string};

#[test]
fn selected_pages_can_exceed_production_json_member_and_string_limits() {
    const PAGE_BUDGET: usize = 512 * 1024;

    let items = (0..4_097).map(|_| string("item".to_owned()).expect("page item")).collect();
    let many_members = object(vec![("items", array(items))]).expect("selected page member count");
    assert!(many_members.canonical_bytes().len() <= PAGE_BUDGET);

    let long_text = object(vec![("content", string("x".repeat(64 * 1024 + 1)))])
        .expect("selected page string length");
    assert!(long_text.canonical_bytes().len() <= PAGE_BUDGET);
}
