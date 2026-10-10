//! Selected-control admission and large native path-authority regression cases.
use super::*;

#[test]
fn selected_features_and_optional_resources_do_not_require_unused_controls() {
    use peritus_sandbox::{ResourceLimits, SandboxFeature};
    use peritus_sandbox_windows::EnforcementLevel;
    use peritus_types::ResourceQuantity;
    let mut evidence = supported_evidence(Sha256Digest::new([0xC3; 32]));
    evidence.conpty = false;
    evidence.credential_manager = false;
    evidence.resources[0] = EnforcementLevel::Unsupported;
    evidence.resources[1] = EnforcementLevel::Unsupported;
    evidence.resources[4] = EnforcementLevel::Unsupported;
    let probe = WindowsProbe::from_evidence(evidence).unwrap();
    assert!(probe.supported_features().contains(SandboxFeature::FilesystemRead));
    assert!(!probe.supported_features().contains(SandboxFeature::Pty));
    assert!(!probe.supported_features().contains(SandboxFeature::CpuTime));
    let quantity = ResourceQuantity::new(100);
    let limits = ResourceLimits::with_optional_wall_output_cpu(
        None, None, None, quantity, quantity, quantity, quantity, quantity,
    )
    .unwrap();
    let plan = support::checked_plan_with_limits(Vec::new(), limits);
    let resources =
        ResourceControlPlan::select_checked_plan(&plan, probe.evidence().resources).unwrap();
    assert_eq!(resources.control(SandboxResourceKind::CpuTime).ceiling(), 0);
    let descriptor = WindowsBackendDescriptor::from_probe(probe, None).unwrap();
    assert!(admit_backend(&plan, descriptor.common(), AdmissionProfile::Production).is_ok());
    let finite = support::checked_plan(Vec::new());
    assert!(
        ResourceControlPlan::select_checked_plan(&finite, descriptor.probe().evidence().resources)
            .is_err()
    );
}

#[test]
fn every_optional_resource_combination_requires_only_selected_native_controls() {
    use peritus_sandbox::{ResourceLimits, SandboxFeature};
    use peritus_sandbox_windows::EnforcementLevel;
    use peritus_types::ResourceQuantity;
    let quantity = ResourceQuantity::new(100);
    for mask in 0_u8..8 {
        let wall = (mask & 1 != 0).then_some(quantity);
        let cpu = (mask & 2 != 0).then_some(quantity);
        let output = (mask & 4 != 0).then_some(quantity);
        let limits = ResourceLimits::with_optional_wall_output_cpu(
            wall, output, cpu, quantity, quantity, quantity, quantity, quantity,
        )
        .unwrap();
        let plan = support::checked_plan_with_limits(Vec::new(), limits);
        let mut evidence = supported_evidence(Sha256Digest::new([0xC3; 32]));
        for (index, feature, selected) in [
            (0, SandboxFeature::WallTime, wall),
            (1, SandboxFeature::CpuTime, cpu),
            (4, SandboxFeature::Output, output),
        ] {
            assert_eq!(plan.required_features().contains(feature), selected.is_some());
            if selected.is_none() {
                evidence.resources[index] = EnforcementLevel::Unsupported;
            }
        }
        let descriptor = WindowsBackendDescriptor::from_probe(
            WindowsProbe::from_evidence(evidence).unwrap(),
            None,
        )
        .unwrap();
        assert!(
            admit_backend(&plan, descriptor.common(), AdmissionProfile::Production).is_ok(),
            "mask {mask}"
        );
        assert!(
            ResourceControlPlan::select_checked_plan(
                &plan,
                descriptor.probe().evidence().resources
            )
            .is_ok()
        );
    }
}

#[test]
fn large_deduplicated_native_roots_and_protected_descendants_keep_deny_precedence() {
    let workspace = WindowsPath::new("C:/workspace").unwrap();
    let protected = WindowsPath::new("C:/workspace/workspace/private").unwrap();
    let mut roots = (0..513)
        .map(|index| WindowsPath::new(format!("D:/private/{index}")).unwrap())
        .collect::<Vec<_>>();
    roots.extend(vec![protected.clone(); 600]);
    let policy = PathPolicy::new(workspace.clone(), roots).unwrap();
    assert_eq!(policy.protected_roots().len(), 514);
    let plan = support::checked_plan(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace/private/file").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Write]),
        )
        .unwrap(),
    ]);
    let acl = compile_acl_plan(&plan, &policy, "S-1-15-2-123").unwrap();
    assert!(
        acl.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Deny && entry.path() == &protected)
    );
    assert!(
        !acl.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Allow && protected.contains(entry.path()))
    );
    let unicode = WindowsPath::new("C:/workspace/Étage").unwrap();
    assert!(unicode.contains(&WindowsPath::new("c:/workspace/étage/file").unwrap()));
    assert!(!unicode.contains(&WindowsPath::new("C:/workspace/Étage-other").unwrap()));
    assert!(
        PathPolicy::new(workspace, vec![unicode, WindowsPath::new("C:/workspace/étage").unwrap()])
            .is_err()
    );
}

#[test]
fn protected_ancestor_is_projected_only_inside_the_configured_authority() {
    let workspace = WindowsPath::new("C:/workspace").unwrap();
    let policy =
        PathPolicy::new(workspace.clone(), vec![WindowsPath::new("C:/").unwrap()]).unwrap();
    let plan =
        compile_acl_plan(&support::checked_plan(Vec::new()), &policy, "S-1-15-2-123").unwrap();
    assert!(plan.entries().iter().all(|entry| workspace.contains(entry.path())));
    assert!(
        plan.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Deny && entry.path() == &workspace)
    );
    assert!(!plan.entries().iter().any(|entry| entry.effect() == RuleEffect::Allow));
}

#[test]
fn a_drive_root_workspace_resolves_logical_components_without_an_empty_component() {
    let policy = PathPolicy::new(WindowsPath::new("C:/").unwrap(), Vec::new()).unwrap();
    assert_eq!(
        policy.resolve_logical(&SandboxPath::new("/bin/tool").unwrap()).unwrap().as_str(),
        "C:/bin/tool"
    );
}
