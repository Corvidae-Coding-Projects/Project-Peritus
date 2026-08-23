use crate::support::{
    FixtureIds, PermissionSpec, deny_rule, descriptor, grant, permission_set, registry, use_limit,
    window,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, CanonicalCollection,
    EnvironmentSelector, OperationClass, PermissionSelector, PolicyDefinition, PolicyErrorKind,
    PolicyTier, RestrictionLayer, RoleSelector, ScopeDimension, ScopeSelector,
};
use peritus_types::{Generation, RevisionNumber, RevisionTuple};

#[derive(Clone, Copy, Debug)]
enum MismatchCase {
    Revision,
    Actor,
    Role,
    Environment,
    Permissions,
}

impl MismatchCase {
    const ALL: [Self; 5] =
        [Self::Revision, Self::Actor, Self::Role, Self::Environment, Self::Permissions];

    const fn dimension(self) -> ScopeDimension {
        match self {
            Self::Revision => ScopeDimension::Revision,
            Self::Actor => ScopeDimension::Actor,
            Self::Role => ScopeDimension::Role,
            Self::Environment => ScopeDimension::Environment,
            Self::Permissions => ScopeDimension::Permissions,
        }
    }
}

const fn inspect(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" }
}

fn boundary(ids: &FixtureIds) -> AuthorityBoundary {
    AuthorityBoundary::new(
        vec![ids.actor],
        vec![ActorRole::Writer],
        vec![ids.environment],
        permission_set(vec![inspect(ids)]),
        ids.revision(),
        window(1, 0, 100),
        use_limit(Some(10)),
    )
    .expect("valid boundary")
}

fn mismatched_revision(ids: &FixtureIds) -> RevisionTuple {
    let revision = ids.revision();
    RevisionTuple::new(
        revision.acceptance_spec_id(),
        revision.harness_id(),
        revision.workspace_id(),
        Generation::new(2).expect("generation"),
        RevisionNumber::first(),
        revision.policy_id(),
        revision.provider_profile_id(),
    )
}

fn selector(ids: &FixtureIds, case: MismatchCase) -> ScopeSelector {
    let revision = if matches!(case, MismatchCase::Revision) {
        mismatched_revision(ids)
    } else {
        ids.revision()
    };
    let actors = if matches!(case, MismatchCase::Actor) {
        ActorSelector::exact(vec![ids.other_actor]).expect("actor selector")
    } else {
        ActorSelector::any_within_parent()
    };
    let roles = if matches!(case, MismatchCase::Role) {
        RoleSelector::exact(vec![ActorRole::Reviewer]).expect("role selector")
    } else {
        RoleSelector::any_within_parent()
    };
    let environments = if matches!(case, MismatchCase::Environment) {
        EnvironmentSelector::exact(vec![ids.other_environment]).expect("environment selector")
    } else {
        EnvironmentSelector::any_within_parent()
    };
    let permissions = if matches!(case, MismatchCase::Permissions) {
        PermissionSelector::exact(permission_set(vec![PermissionSpec {
            resource: ids.third_resource,
            name: "workspace.inspect",
        }]))
    } else {
        PermissionSelector::any_within_parent()
    };
    ScopeSelector::new(actors, roles, environments, permissions, revision)
}

fn assert_selector_error(error: peritus_policy::PolicyError, case: MismatchCase) {
    assert_eq!(error.kind(), PolicyErrorKind::SelectorOutsideBoundary, "{case:?}");
    assert_eq!(error.dimension(), Some(case.dimension()), "{case:?}");
    assert_eq!(error.collection(), None, "{case:?}");
}

#[test]
fn ceiling_reports_the_exact_first_selector_containment_dimension() {
    let ids = FixtureIds::new();
    for case in MismatchCase::ALL {
        let error = AuthorityCeiling::new(
            boundary(&ids),
            vec![grant(10, selector(&ids, case), window(1, 0, 100), use_limit(Some(2)))],
            Vec::new(),
        )
        .expect_err("mismatched grant must fail");
        assert_selector_error(error, case);
    }
}

#[test]
fn policy_reports_the_exact_first_rule_containment_dimension() {
    let ids = FixtureIds::new();
    for case in MismatchCase::ALL {
        let ceiling = AuthorityCeiling::new(boundary(&ids), Vec::new(), Vec::new())
            .expect("valid empty ceiling");
        let layer =
            RestrictionLayer::new(PolicyTier::User, vec![deny_rule(20, selector(&ids, case))])
                .expect("canonical layer");
        let error = PolicyDefinition::new(
            ids.policy,
            ceiling,
            registry(vec![descriptor("workspace.inspect", OperationClass::Inspection)]),
            vec![layer],
        )
        .expect_err("mismatched rule must fail");
        assert_selector_error(error, case);
    }
}

#[test]
fn duplicate_grants_report_the_grants_collection() {
    let ids = FixtureIds::new();
    let make_grant = || {
        grant(
            10,
            crate::support::any_selector(ids.revision()),
            window(1, 0, 100),
            use_limit(Some(2)),
        )
    };
    let error = AuthorityCeiling::new(boundary(&ids), vec![make_grant(), make_grant()], Vec::new())
        .expect_err("duplicate grants must fail");
    assert_eq!(error.kind(), PolicyErrorKind::DuplicateCanonicalValue);
    assert_eq!(error.collection(), Some(CanonicalCollection::Grants));
    assert_eq!(error.dimension(), None);
}
