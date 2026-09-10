use super::*;
use crate::{AppResponsePayload, WorkbenchInputRow, WorkbenchInputState};

fn fixture() -> WorkbenchImagePage {
    let case = crate::schema::generated_fixture_cases()
        .expect("fixtures")
        .into_iter()
        .find(|case| case.case == "realistic-workbench-image-page")
        .expect("page");
    let AppMessage::Response(response) =
        decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).expect("decode")
    else {
        panic!("response")
    };
    let AppResponsePayload::WorkbenchImages(page) = response.payload() else { panic!("page") };
    page.clone()
}

#[test]
fn image_page_requires_exact_cardinality_unique_operations_and_committed_revision() {
    let page = fixture();
    let row = page.rows()[0].clone();
    assert!(WorkbenchImagePage::new(page.query(), 2, vec![row.clone(), row.clone()]).is_err());
    assert!(WorkbenchImagePage::new(page.query(), 2, vec![row.clone()]).is_err());
    assert!(WorkbenchImagePage::new(page.query(), 1, page.rows().to_vec()).is_err());
    assert!(WorkbenchImagePage::new(page.query(), 257, vec![]).is_err());
    let unfenced = WorkbenchImageQuery::new(page.query().query(), 0, 0).expect("latest request");
    assert!(WorkbenchImagePage::new(unfenced, 1, vec![row]).is_err());
    let end = WorkbenchImageQuery::new(page.query().query(), 12, 2).expect("at end");
    assert!(WorkbenchImagePage::new(end, 2, vec![]).is_ok());
    let after = WorkbenchImageQuery::new(page.query().query(), 12, 3).expect("after");
    assert!(WorkbenchImagePage::new(after, 2, vec![]).is_err());
}

#[test]
fn image_eligibility_rejects_deselected_held_withdrawn_and_superseded_sources() {
    let page = fixture();
    let row = &page.rows()[0];
    for (state, selection, valid) in [
        (WorkbenchInputState::Queued, (false, true), false),
        (WorkbenchInputState::Held, (true, true), false),
        (WorkbenchInputState::Withdrawn, (true, true), false),
        (WorkbenchInputState::Superseded, (false, false), false),
        (WorkbenchInputState::Held, (true, false), true),
        (WorkbenchInputState::Withdrawn, (true, false), true),
        (WorkbenchInputState::Incorporated, (true, true), true),
    ] {
        let source = WorkbenchInputRow::new(
            row.source().selected(),
            row.source().text().clone(),
            state,
            row.source().dependencies().clone(),
        )
        .expect("source");
        let result = WorkbenchImageRow::new(
            row.operation(),
            row.artifact(),
            row.label().clone(),
            row.image(),
            source,
            selection,
        );
        assert_eq!(result.is_ok(), valid, "{state:?} {selection:?}");
    }
}
