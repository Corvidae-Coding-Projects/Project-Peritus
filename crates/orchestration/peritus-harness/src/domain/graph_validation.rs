//! Focused graph algorithms and graph-canonical helpers.

use crate::domain::{
    AuthoritySet, CanonicalEncoder, CanonicalReader, ComponentDeclaration, ComponentId, FeatureTag,
    GraphEnvironment, HarnessDomainError, HarnessDomainErrorKind, HarnessLimitKind, HarnessLimits,
    ProtectedAsset, ProtectionClass, ResolvedEdge,
};

const DECLARATION_DOMAIN: &[u8] = b"peritus-e1-component-declaration-v1\0";

pub(super) fn validate_graph_bounds(
    declarations: &[ComponentDeclaration],
    limits: HarnessLimits,
) -> Result<(), HarnessDomainError> {
    if declarations.is_empty() {
        return Err(HarnessDomainError::detail(
            HarnessDomainErrorKind::EmptyValue,
            "checked harness graph contains no components",
        ));
    }
    if u64_len(declarations.len()) > limits.max_components() {
        return Err(HarnessDomainError::limit(
            HarnessDomainErrorKind::TooManyComponents,
            HarnessLimitKind::Components,
            limits.max_components(),
            u64_len(declarations.len()),
        ));
    }
    let mut edges = 0_u64;
    let mut total_bytes = 0_u64;
    for declaration in declarations {
        let dependency_count = u64_len(declaration.dependencies().len());
        if dependency_count > limits.max_dependency_fan_out() {
            return Err(HarnessDomainError::component_numbers(
                HarnessDomainErrorKind::TooManyDependencies,
                declaration.id().clone(),
                limits.max_dependency_fan_out(),
                dependency_count,
            ));
        }
        if declaration.byte_length() > limits.max_component_bytes() {
            return Err(HarnessDomainError::component_numbers(
                HarnessDomainErrorKind::ComponentTooLarge,
                declaration.id().clone(),
                limits.max_component_bytes(),
                declaration.byte_length(),
            ));
        }
        edges = edges
            .checked_add(dependency_count)
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::ArithmeticOverflow))?;
        total_bytes = total_bytes
            .checked_add(declaration.byte_length())
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::ArithmeticOverflow))?;
    }
    if edges > limits.max_dependency_edges() {
        return Err(HarnessDomainError::limit(
            HarnessDomainErrorKind::TooManyDependencyEdges,
            HarnessLimitKind::DependencyEdges,
            limits.max_dependency_edges(),
            edges,
        ));
    }
    if total_bytes > limits.max_total_materialized_bytes() {
        return Err(HarnessDomainError::limit(
            HarnessDomainErrorKind::TotalBytesExceeded,
            HarnessLimitKind::TotalMaterializedBytes,
            limits.max_total_materialized_bytes(),
            total_bytes,
        ));
    }
    Ok(())
}

pub(super) fn validate_unique_declarations(
    declarations: &[ComponentDeclaration],
) -> Result<(), HarnessDomainError> {
    for pair in declarations.windows(2) {
        if pair[0].id() == pair[1].id() {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::DuplicateComponent,
                pair[1].id().clone(),
            ));
        }
    }
    for (index, left) in declarations.iter().enumerate() {
        for right in &declarations[index + 1..] {
            if left.source_path() == right.source_path() {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::DuplicateSourcePath,
                    left.id().clone(),
                    right.id().clone(),
                ));
            }
            if left.target_path() == right.target_path() {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::DuplicateTargetPath,
                    left.id().clone(),
                    right.id().clone(),
                ));
            }
            if paths_collide(left.target_path().as_str(), right.target_path().as_str()) {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::TargetPathCollision,
                    left.id().clone(),
                    right.id().clone(),
                ));
            }
        }
    }
    Ok(())
}

fn paths_collide(left: &str, right: &str) -> bool {
    left.strip_prefix(right).is_some_and(|suffix| suffix.starts_with('/'))
        || right.strip_prefix(left).is_some_and(|suffix| suffix.starts_with('/'))
}

type ResolvedRequirements = (Vec<ResolvedEdge>, Vec<FeatureTag>, Vec<FeatureTag>);

pub(super) fn resolve_requirements(
    declarations: &[ComponentDeclaration],
    environment: &GraphEnvironment,
) -> Result<ResolvedRequirements, HarnessDomainError> {
    let mut edges = Vec::new();
    let mut provider_features = Vec::new();
    let mut platform_features = Vec::new();
    for declaration in declarations {
        check_features(declaration, environment, &mut provider_features, &mut platform_features)?;
        for requirement in declaration.dependencies() {
            let dependency = declarations
                .binary_search_by(|item| item.id().cmp(requirement.component_id()))
                .ok()
                .map(|index| &declarations[index])
                .ok_or_else(|| {
                    HarnessDomainError::components(
                        HarnessDomainErrorKind::MissingDependency,
                        declaration.id().clone(),
                        requirement.component_id().clone(),
                    )
                })?;
            if dependency.kind() != requirement.required_kind() {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::IncompatibleDependencyKind,
                    declaration.id().clone(),
                    dependency.id().clone(),
                ));
            }
            if !requirement.compatible_schema().contains(dependency.schema_version()) {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::IncompatibleDependencyVersion,
                    declaration.id().clone(),
                    dependency.id().clone(),
                ));
            }
            if requirement
                .exact_content_digest()
                .is_some_and(|digest| digest != dependency.content_digest())
            {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::DependencyDigestMismatch,
                    declaration.id().clone(),
                    dependency.id().clone(),
                ));
            }
            if declaration.protection_class() == ProtectionClass::Evolvable
                && dependency.protection_class().is_protected()
                && !declaration.kind().accepts_protected_dependency(dependency.protection_class())
            {
                return Err(HarnessDomainError::components(
                    HarnessDomainErrorKind::ProtectedDependency,
                    declaration.id().clone(),
                    dependency.id().clone(),
                ));
            }
            edges.push(ResolvedEdge {
                depender: declaration.id().clone(),
                dependency: dependency.id().clone(),
            });
        }
    }
    Ok((edges, provider_features, platform_features))
}

fn check_features(
    declaration: &ComponentDeclaration,
    environment: &GraphEnvironment,
    provider_features: &mut Vec<FeatureTag>,
    platform_features: &mut Vec<FeatureTag>,
) -> Result<(), HarnessDomainError> {
    for feature in declaration.compatibility().provider_features() {
        if environment.provider_features.binary_search(feature).is_err() {
            return Err(HarnessDomainError::component_detail(
                HarnessDomainErrorKind::UnsatisfiedProviderFeature,
                declaration.id().clone(),
                feature.as_str(),
            ));
        }
        insert_sorted_unique(provider_features, feature.clone());
    }
    for feature in declaration.compatibility().platform_features() {
        if environment.platform_features.binary_search(feature).is_err() {
            return Err(HarnessDomainError::component_detail(
                HarnessDomainErrorKind::UnsatisfiedPlatformFeature,
                declaration.id().clone(),
                feature.as_str(),
            ));
        }
        insert_sorted_unique(platform_features, feature.clone());
    }
    Ok(())
}

pub(super) fn topological_order(
    declarations: &[ComponentDeclaration],
) -> Result<Vec<ComponentId>, HarnessDomainError> {
    let mut indegrees: Vec<usize> =
        declarations.iter().map(|item| item.dependencies().len()).collect();
    let mut resolved = vec![false; declarations.len()];
    let mut order = Vec::with_capacity(declarations.len());
    while order.len() < declarations.len() {
        let next = (0..declarations.len()).find(|&index| !resolved[index] && indegrees[index] == 0);
        let Some(index) = next else {
            let member = declarations
                .iter()
                .enumerate()
                .find(|(at, _)| !resolved[*at])
                .map(|(_, item)| item.id().clone())
                .ok_or_else(|| {
                    HarnessDomainError::plain(HarnessDomainErrorKind::DependencyCycle)
                })?;
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::DependencyCycle,
                member,
            ));
        };
        resolved[index] = true;
        order.push(declarations[index].id().clone());
        for (consumer_index, consumer) in declarations.iter().enumerate() {
            if !resolved[consumer_index]
                && consumer
                    .dependencies()
                    .iter()
                    .any(|dependency| dependency.component_id() == declarations[index].id())
            {
                indegrees[consumer_index] =
                    indegrees[consumer_index].checked_sub(1).ok_or_else(|| {
                        HarnessDomainError::plain(HarnessDomainErrorKind::ArithmeticOverflow)
                    })?;
            }
        }
    }
    Ok(order)
}

pub(super) fn validate_authority_closures(
    declarations: &[ComponentDeclaration],
    order: &[ComponentId],
) -> Result<AuthoritySet, HarnessDomainError> {
    let mut closures = vec![AuthoritySet::empty(); declarations.len()];
    let mut aggregate = AuthoritySet::empty();
    for id in order {
        let index = declarations.binary_search_by(|item| item.id().cmp(id)).map_err(|_| {
            HarnessDomainError::component(HarnessDomainErrorKind::MissingDependency, id.clone())
        })?;
        let declaration = &declarations[index];
        let mut closure = declaration.declared_authority();
        for dependency in declaration.dependencies() {
            let dependency_index = declarations
                .binary_search_by(|item| item.id().cmp(dependency.component_id()))
                .map_err(|_| {
                    HarnessDomainError::components(
                        HarnessDomainErrorKind::MissingDependency,
                        declaration.id().clone(),
                        dependency.component_id().clone(),
                    )
                })?;
            closure = closure.union(closures[dependency_index]);
        }
        if !closure.is_subset_of(declaration.kind().authority_ceiling()) {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::DependencyAuthorityExceeded,
                declaration.id().clone(),
            ));
        }
        closures[index] = closure;
        aggregate = aggregate.union(closure);
    }
    Ok(aggregate)
}

pub(super) fn protected_inventory(
    declarations: &[ComponentDeclaration],
    order: &[ComponentId],
) -> Vec<ProtectedAsset> {
    order
        .iter()
        .enumerate()
        .filter_map(|(position, id)| {
            let declaration = declarations.iter().find(|item| item.id() == id)?;
            declaration.protection_class().is_protected().then(|| {
                let mut encoder = CanonicalEncoder::new(DECLARATION_DOMAIN);
                declaration.encode(&mut encoder);
                ProtectedAsset {
                    component_id: id.clone(),
                    declaration_position: u64_len(position),
                    protection_class: declaration.protection_class(),
                    declaration_digest: peritus_codec::sha256(&encoder.into_bytes()),
                }
            })
        })
        .collect()
}

fn insert_sorted_unique(values: &mut Vec<FeatureTag>, value: FeatureTag) {
    match values.binary_search(&value) {
        Ok(_) => {}
        Err(index) => values.insert(index, value),
    }
}

pub(super) fn validate_order(
    values: &[FeatureTag],
    family: &'static str,
) -> Result<(), HarnessDomainError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(HarnessDomainError::detail(
            HarnessDomainErrorKind::NonCanonicalOrder,
            format!("{family} features are not in strict canonical order"),
        ))
    } else {
        Ok(())
    }
}

pub(super) fn u64_len(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

pub(super) fn encode_features(encoder: &mut CanonicalEncoder, features: &[FeatureTag]) {
    encoder.len(features.len());
    for feature in features {
        encoder.string(feature.as_str());
    }
}

pub(super) fn decode_features(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<Vec<FeatureTag>, HarnessDomainError> {
    let count = reader.length()?;
    if u64_len(count) > limits.max_dependency_edges() {
        return Err(HarnessDomainError::limit(
            HarnessDomainErrorKind::TooManyDependencyEdges,
            HarnessLimitKind::DependencyEdges,
            limits.max_dependency_edges(),
            u64_len(count),
        ));
    }
    let mut features = Vec::with_capacity(count);
    for _ in 0..count {
        features.push(FeatureTag::new(reader.string()?)?);
    }
    validate_order(&features, "decoded")?;
    Ok(features)
}

pub(super) fn encode_limits(encoder: &mut CanonicalEncoder, limits: HarnessLimits) {
    for kind in LIMIT_KINDS {
        encoder.u64(limits.value(kind));
    }
}

pub(super) fn decode_limits(
    reader: &mut CanonicalReader<'_>,
) -> Result<HarnessLimits, HarnessDomainError> {
    let mut limits = HarnessLimits::compiled();
    for kind in LIMIT_KINDS {
        limits = limits.tighten(kind, reader.u64()?)?;
    }
    Ok(limits)
}

const LIMIT_KINDS: [HarnessLimitKind; 11] = [
    HarnessLimitKind::ManifestBytes,
    HarnessLimitKind::Components,
    HarnessLimitKind::DependencyEdges,
    HarnessLimitKind::DependencyFanOut,
    HarnessLimitKind::ComponentBytes,
    HarnessLimitKind::TotalMaterializedBytes,
    HarnessLimitKind::RevisionHistory,
    HarnessLimitKind::ReceiptHistory,
    HarnessLimitKind::EventBytes,
    HarnessLimitKind::StateBytes,
    HarnessLimitKind::RetainedDiagnostics,
];
