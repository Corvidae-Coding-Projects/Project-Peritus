use crate::error::Diagnostic;
use crate::model::{
    ArchitecturePolicy, CargoDependency, CargoDependencyKind, CargoPackage, PackagePolicy,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn validate_dependency_edges(
    root: &Path,
    policy: &ArchitecturePolicy,
    packages: &[&CargoPackage],
    cargo_by_name: &BTreeMap<&str, &CargoPackage>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let policy_by_name: BTreeMap<_, _> =
        policy.packages.iter().map(|package| (package.name.as_str(), package)).collect();
    let mut production_graph = BTreeMap::new();
    for package in packages {
        let Some(source_policy) = policy_by_name.get(package.name.as_str()) else { continue };
        let manifest = super::relative(root, &package.manifest_path);
        let mut production_targets = Vec::new();
        for dependency in &package.dependencies {
            let Some(target) = cargo_by_name.get(dependency.name.as_str()) else { continue };
            let Some(target_policy) = policy_by_name.get(target.name.as_str()) else { continue };
            validate_dependency_edge(
                policy,
                source_policy,
                target_policy,
                dependency,
                &manifest,
                diagnostics,
            );
            if dependency.kind != Some(CargoDependencyKind::Development) {
                production_targets.push(target.name.clone());
            }
        }
        production_graph.insert(package.name.clone(), production_targets);
    }
    if let Some(cycle) = find_cycle(&production_graph) {
        diagnostics.push(Diagnostic::new(
            format!(
                "production/build package dependency graph contains a cycle: {}",
                cycle.join(" -> ")
            ),
            "break the cycle with an inward protocol; development-only edges are deliberately excluded",
        ));
    }
}

fn validate_dependency_edge(
    policy: &ArchitecturePolicy,
    source: &PackagePolicy,
    target: &PackagePolicy,
    dependency: &CargoDependency,
    manifest: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let development = dependency.kind == Some(CargoDependencyKind::Development);
    let edge = describe_edge(dependency);
    let allowed_layers = policy
        .layers
        .iter()
        .find(|layer| layer.name == source.layer)
        .map(|layer| if development { &layer.may_dev_depend_on } else { &layer.may_depend_on });
    if allowed_layers.is_none_or(|allowed| !allowed.contains(&target.layer)) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "layer `{}` may not use {edge} on `{}` layer through package `{}`",
                source.layer, target.layer, target.name
            ),
            "invert the dependency, introduce an inward protocol, or amend the matching reviewed edge policy",
        ));
    }
    let allowed_classes = policy
        .verification_classes
        .iter()
        .find(|class| class.name == source.verification_class)
        .map(|class| if development { &class.may_dev_depend_on } else { &class.may_depend_on });
    if allowed_classes.is_none_or(|allowed| !allowed.contains(&target.verification_class)) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "verification class `{}` may not use {edge} on class `{}` package `{}`",
                source.verification_class, target.verification_class, target.name
            ),
            "depend on a permitted class boundary or revise the reviewed verification-class matrix",
        ));
    }
    if let Some(forbidden) = policy
        .forbidden_dependencies
        .iter()
        .find(|pair| pair.from == source.name && pair.to == target.name)
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "package `{}` may not use {edge} on forbidden package `{}`: {}",
                source.name, target.name, forbidden.rationale
            ),
            "remove the dependency or change the reviewed package-pair prohibition through architecture review",
        ));
    }
}

fn describe_edge(dependency: &CargoDependency) -> String {
    let kind = match dependency.kind {
        None => "normal",
        Some(CargoDependencyKind::Build) => "build",
        Some(CargoDependencyKind::Development) => "development",
    };
    let optional = if dependency.optional { ", optional" } else { "" };
    let target = dependency
        .target
        .as_deref()
        .map_or_else(String::new, |target| format!(", target `{target}`"));
    format!("{kind} dependency{optional}{target}")
}

pub(super) fn find_cycle(graph: &BTreeMap<String, Vec<String>>) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        complete: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if let Some(start) = stack.iter().position(|active| active == node) {
            let mut cycle = stack[start..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        if complete.contains(node) {
            return None;
        }
        stack.push(node.to_owned());
        if let Some(targets) = graph.get(node) {
            for target in targets.iter().filter(|target| graph.contains_key(*target)) {
                if let Some(cycle) = visit(target, graph, complete, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        complete.insert(node.to_owned());
        None
    }

    let mut complete = BTreeSet::new();
    for node in graph.keys() {
        if let Some(cycle) = visit(node, graph, &mut complete, &mut Vec::new()) {
            return Some(cycle);
        }
    }
    None
}
