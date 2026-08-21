use super::dependency::find_cycle;
use crate::error::Diagnostic;
use crate::model::ArchitecturePolicy;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub(super) fn validate_policy(policy: &ArchitecturePolicy) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_schema_and_budgets(policy, &mut diagnostics);
    let layer_names = validate_layers(policy, &mut diagnostics);
    let class_names = validate_classes(policy, &mut diagnostics);
    let package_names = validate_packages(policy, &layer_names, &class_names, &mut diagnostics);
    validate_forbidden_pairs(policy, &package_names, &mut diagnostics);
    validate_controlled_roots(policy, &mut diagnostics);
    validate_trusted_source_roots(policy, &mut diagnostics);
    validate_source_exceptions(policy, &mut diagnostics);
    diagnostics
}

fn validate_trusted_source_roots(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    let mut roots = BTreeSet::new();
    for trusted in &policy.trusted_source_roots {
        let structurally_safe = !trusted.as_os_str().is_empty()
            && !trusted.is_absolute()
            && trusted.components().all(|component| matches!(component, Component::Normal(_)));
        if !structurally_safe {
            diagnostics.push(Diagnostic::at(
                trusted,
                "trusted source root must be a non-empty repository-relative normal path",
                "use a narrow path with no root, current-directory, or parent-directory components",
            ));
            continue;
        }
        if !roots.insert(trusted) {
            diagnostics.push(Diagnostic::at(
                trusted,
                "trusted source root is declared more than once",
                "retain one reviewed declaration for each trusted boundary",
            ));
        }
        if policy.ignored_directories.iter().map(Path::new).any(|ignored| {
            is_normal_relative_path(ignored)
                && (trusted.starts_with(ignored) || ignored.starts_with(trusted))
        }) {
            diagnostics.push(Diagnostic::at(
                trusted,
                "trusted source root overlaps an ignored repository prefix",
                "place audited trusted code in an unignored source tree so every occurrence is scanned",
            ));
        }
    }

    let roots: Vec<_> = roots.into_iter().collect();
    for (index, root) in roots.iter().enumerate() {
        if roots[index + 1..].iter().any(|other| root.starts_with(other) || other.starts_with(root))
        {
            diagnostics.push(Diagnostic::at(
                root,
                "trusted source roots overlap",
                "retain one narrow, non-overlapping ownership root per audited trust boundary",
            ));
        }
    }
}

fn is_normal_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_)))
}

fn validate_schema_and_budgets(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    if policy.schema != 2 {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            format!("unsupported schema {}; expected 2", policy.schema),
            "migrate the policy deliberately before changing its schema number",
        ));
    }
    if policy.soft_source_lines == 0 || policy.hard_source_lines < policy.soft_source_lines {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            "source line budgets are inconsistent",
            "set a positive soft limit no greater than the hard limit",
        ));
    }
    if policy.root_module_lines == 0 || policy.root_module_lines > policy.soft_source_lines {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            "root-module budget must be positive and no greater than the source soft limit",
            "keep crate roots small composition surfaces",
        ));
    }
}

fn validate_layers<'a>(
    policy: &'a ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<&'a str> {
    let mut layer_names = BTreeSet::new();
    let mut layer_paths = BTreeSet::new();
    let mut layer_graph = BTreeMap::new();
    for layer in &policy.layers {
        if !layer_names.insert(layer.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!("dependency layer `{}` is declared more than once", layer.name),
                "give every dependency layer one unique name",
            ));
        }
        if !layer_paths.insert(&layer.path) {
            diagnostics.push(Diagnostic::at(
                &layer.path,
                "more than one dependency layer owns this path",
                "assign each physical group directory to exactly one layer",
            ));
        }
        layer_graph.insert(
            layer.name.clone(),
            layer.may_depend_on.iter().filter(|target| *target != &layer.name).cloned().collect(),
        );
    }
    for layer in &policy.layers {
        validate_references(
            "layer",
            &layer.name,
            "production/build",
            &layer.may_depend_on,
            &layer_names,
            diagnostics,
        );
        validate_references(
            "layer",
            &layer.name,
            "development",
            &layer.may_dev_depend_on,
            &layer_names,
            diagnostics,
        );
    }
    if let Some(cycle) = find_cycle(&layer_graph) {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            format!("dependency layer policy contains a cycle: {}", cycle.join(" -> ")),
            "remove a mutual layer allowance so production dependencies have one inward direction",
        ));
    }
    layer_names
}

fn validate_classes<'a>(
    policy: &'a ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<&'a str> {
    let mut class_names = BTreeSet::new();
    for class in &policy.verification_classes {
        if !class_names.insert(class.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!("verification class `{}` is declared more than once", class.name),
                "keep one reviewed dependency rule for each verification class",
            ));
        }
    }
    if class_names != BTreeSet::from(["V", "H", "T", "C"]) {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            "verification-class policy must define exactly V, H, T, and C",
            "declare an explicit production and development dependency matrix for all four classes",
        ));
    }
    for class in &policy.verification_classes {
        validate_references(
            "verification class",
            &class.name,
            "production/build",
            &class.may_depend_on,
            &class_names,
            diagnostics,
        );
        validate_references(
            "verification class",
            &class.name,
            "development",
            &class.may_dev_depend_on,
            &class_names,
            diagnostics,
        );
    }
    class_names
}

fn validate_packages<'a>(
    policy: &'a ArchitecturePolicy,
    layer_names: &BTreeSet<&str>,
    class_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<&'a str> {
    for layer in &policy.layers {
        if let Some(required_class) = &layer.required_verification_class
            && !class_names.contains(required_class.as_str())
        {
            diagnostics.push(Diagnostic::at(
                &layer.path,
                format!(
                    "layer `{}` requires unknown verification class `{required_class}`",
                    layer.name
                ),
                "require one of the verification classes declared by this architecture policy",
            ));
        }
    }

    let mut package_names = BTreeSet::new();
    let mut package_paths = BTreeSet::new();
    for package in &policy.packages {
        if !package_names.insert(package.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!("package `{}` is registered more than once", package.name),
                "retain one canonical ownership record per Cargo package",
            ));
        }
        if !package_paths.insert(&package.path) {
            diagnostics.push(Diagnostic::at(
                &package.path,
                "more than one package is registered at this path",
                "assign every package directory to exactly one owner",
            ));
        }
        if !layer_names.contains(package.layer.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!("package `{}` names unknown layer `{}`", package.name, package.layer),
                "register the layer before assigning packages to it",
            ));
            continue;
        }
        let layer = policy
            .layers
            .iter()
            .find(|layer| layer.name == package.layer)
            .expect("known layer was collected from this policy");
        if !package.path.starts_with(&layer.path) {
            diagnostics.push(Diagnostic::at(
                &package.path,
                format!("package is outside its `{}` layer path", package.layer),
                format!(
                    "move it under {} or correct the reviewed layer assignment",
                    layer.path.display()
                ),
            ));
        }
        if let Some(physical_layer) = policy
            .layers
            .iter()
            .filter(|candidate| package.path.starts_with(&candidate.path))
            .max_by_key(|candidate| candidate.path.components().count())
        {
            if physical_layer.name != package.layer {
                diagnostics.push(Diagnostic::at(
                    &package.path,
                    format!(
                        "package is registered in `{}` but its most specific physical layer is `{}`",
                        package.layer, physical_layer.name
                    ),
                    "assign nested packages to the narrowest reviewed layer so dependency policy cannot be bypassed",
                ));
            }
            if let Some(required_class) = &physical_layer.required_verification_class
                && package.verification_class != *required_class
            {
                diagnostics.push(Diagnostic::at(
                    &package.path,
                    format!(
                        "physical layer `{}` requires verification class `{required_class}`, not `{}`",
                        physical_layer.name, package.verification_class
                    ),
                    "use the layer's required class so test-only support cannot masquerade as verified production code",
                ));
            }
        }
        if package.owner.trim().is_empty() {
            diagnostics.push(Diagnostic::new(
                format!("package `{}` has no owner", package.name),
                "record the owning implementation slice",
            ));
        }
        if !class_names.contains(package.verification_class.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!(
                    "package `{}` has invalid verification class `{}`",
                    package.name, package.verification_class
                ),
                "use one of V, H, T, or C",
            ));
        }
    }
    package_names
}

fn validate_forbidden_pairs(
    policy: &ArchitecturePolicy,
    package_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut pairs = BTreeSet::new();
    for pair in &policy.forbidden_dependencies {
        if !pairs.insert((pair.from.as_str(), pair.to.as_str())) {
            diagnostics.push(Diagnostic::new(
                format!("forbidden dependency `{} -> {}` is duplicated", pair.from, pair.to),
                "retain one reviewed prohibition and rationale for the package pair",
            ));
        }
        if !package_names.contains(pair.from.as_str()) || !package_names.contains(pair.to.as_str())
        {
            diagnostics.push(Diagnostic::new(
                format!(
                    "forbidden dependency `{} -> {}` names an unregistered package",
                    pair.from, pair.to
                ),
                "register both packages or remove the stale prohibition",
            ));
        }
        if pair.rationale.trim().len() < 20 {
            diagnostics.push(Diagnostic::new(
                format!(
                    "forbidden dependency `{} -> {}` lacks a substantive rationale",
                    pair.from, pair.to
                ),
                "document the authority, trust, or maintenance boundary being protected",
            ));
        }
    }
}

fn validate_controlled_roots(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    let mut paths = BTreeSet::new();
    for controlled in &policy.controlled_source_roots {
        if controlled.path.as_os_str().is_empty()
            || controlled.path.is_absolute()
            || !paths.insert(&controlled.path)
        {
            diagnostics.push(Diagnostic::at(
                &controlled.path,
                "controlled generated/schema root must be unique and repository-relative",
                "use one non-empty repository-relative path for each owned generated/schema tree",
            ));
        }
        if controlled.owner.trim().is_empty() || controlled.rationale.trim().len() < 20 {
            diagnostics.push(Diagnostic::at(
                &controlled.path,
                "controlled generated/schema root lacks an owner or substantive rationale",
                "name the accountable slice and document the generation or compatibility contract",
            ));
        }
    }
}

fn validate_source_exceptions(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    let mut paths = BTreeSet::new();
    for exception in &policy.source_exceptions {
        if !paths.insert(&exception.path) {
            diagnostics.push(Diagnostic::at(
                &exception.path,
                "source-size exception is duplicated",
                "keep one reviewed exception with a single accountable owner",
            ));
        }
        if exception.owner.trim().is_empty() || exception.rationale.trim().len() < 20 {
            diagnostics.push(Diagnostic::at(
                &exception.path,
                "source-size exception lacks an owner or substantive rationale",
                "name an owner and explain why decomposition would harm cohesion",
            ));
        }
    }
}

fn validate_references(
    kind: &str,
    name: &str,
    edge_kind: &str,
    targets: &[String],
    known: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for target in targets {
        if !known.contains(target.as_str()) {
            diagnostics.push(Diagnostic::new(
                format!("{kind} `{name}` allows unknown {edge_kind} {kind} `{target}`"),
                "declare the target or remove the stale dependency edge",
            ));
        }
    }
}
