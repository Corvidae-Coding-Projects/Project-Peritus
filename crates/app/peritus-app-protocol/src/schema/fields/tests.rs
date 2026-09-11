use std::collections::BTreeSet;

use super::*;

#[test]
fn review_intents_are_closed_union_members_with_resolved_json_refs() {
    let descriptors = APP_NESTED_TYPES
        .iter()
        .chain(APP_FLOW_TYPES)
        .flat_map(|group| group.iter())
        .collect::<Vec<_>>();
    let mut names = descriptors.iter().map(|descriptor| descriptor.name).collect::<BTreeSet<_>>();
    names.insert("AppErrorCode");

    for descriptor in &descriptors {
        for field in descriptor.fields {
            let assert_resolved = |reference: &str| {
                assert!(
                    names.contains(reference),
                    "{}.{}, unresolved JSON ref {reference}",
                    descriptor.name,
                    field.name,
                );
            };
            match field.json_shape {
                JsonShape::Ref(name) | JsonShape::ArrayRef(name) => assert_resolved(name),
                JsonShape::OneOfRef(references) | JsonShape::OneOfArrayRef(references) => {
                    for reference in references {
                        assert_resolved(reference);
                    }
                }
                _ => {}
            }
        }
    }

    let command = descriptors
        .iter()
        .find(|descriptor| descriptor.name == "WorkbenchCommand")
        .expect("workbench command descriptor");
    let intent =
        command.fields.iter().find(|field| field.name == "intent").expect("workbench intent union");
    let JsonShape::OneOfRef(members) = intent.json_shape else {
        panic!("workbench intent must be a closed union");
    };
    for expected in
        ["WorkbenchAddReviewIntent", "WorkbenchRebindReviewIntent", "WorkbenchDismissReviewIntent"]
    {
        assert!(members.contains(&expected), "missing JSON union member {expected}");
        assert!(
            intent.typescript_type.split(" | ").any(|member| member == expected),
            "missing TypeScript union member {expected}",
        );
    }
}
