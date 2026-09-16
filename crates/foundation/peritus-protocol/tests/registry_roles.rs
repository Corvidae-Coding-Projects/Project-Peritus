//! Regression coverage for the stable command and event family role registry.

use peritus_protocol::schema::{FAMILIES, MessageRole};

#[test]
fn registered_command_and_event_roles_are_complete_and_disjoint() {
    let commands = FAMILIES
        .iter()
        .filter(|family| family.role() == MessageRole::Command)
        .map(|family| family.tag)
        .collect::<Vec<_>>();
    let events = FAMILIES
        .iter()
        .filter(|family| family.role() == MessageRole::Event)
        .map(|family| family.tag)
        .collect::<Vec<_>>();

    assert_eq!(commands, [1, 10, 40, 50, 53, 70, 73, 76, 79, 82, 85, 88, 91]);
    assert_eq!(events, [3, 41, 51, 54, 60, 71, 74, 77, 80, 83, 86, 89, 92, 94, 3500, 65_000]);
    assert!(commands.iter().all(|tag| !events.contains(tag)));
}

#[test]
fn every_registered_family_has_one_stable_role() {
    for family in FAMILIES {
        match family.role() {
            MessageRole::Command
            | MessageRole::CommandEnvelope
            | MessageRole::Event
            | MessageRole::State
            | MessageRole::Record => {}
        }
    }
    assert_eq!(
        FAMILIES.iter().find(|family| family.tag == 2).map(|family| family.role()),
        Some(MessageRole::CommandEnvelope)
    );
}

#[test]
fn scheduler_roles_accept_both_registered_schema_versions() {
    for (tag, role) in
        [(70, MessageRole::Command), (71, MessageRole::Event), (72, MessageRole::State)]
    {
        let family = FAMILIES.iter().find(|family| family.tag == tag).expect("scheduler family");
        assert_eq!(family.role(), role);
        assert_eq!(family.schema_version, 2);
        assert_eq!(family.supported_schema_versions, &[1, 2]);
        assert!(family.supports(1));
        assert!(family.supports(2));
        assert!(!family.supports(0));
        assert!(!family.supports(3));
    }
}
