//! Exact allowlist selection and B1 operation construction.

use std::collections::BTreeSet;

use peritus_policy::{OperationDescriptor, OperationRegistry, PolicyError, RiskSet};

use super::{
    ToolComponentError, ToolComponentErrorKind, ToolRegistration, catalog::ToolDeclaration,
};

pub(super) fn checked_names(allowed: &[String]) -> Result<BTreeSet<&str>, ToolComponentError> {
    let mut names = BTreeSet::new();
    for name in allowed {
        if !names.insert(name.as_str()) {
            return Err(ToolComponentError::new(
                ToolComponentErrorKind::DuplicateTool,
                "construct configured tool inventory",
                format!("tool {name} is configured more than once"),
            ));
        }
    }
    Ok(names)
}

pub(super) fn select(
    catalog: Vec<ToolDeclaration>,
    allowed: &BTreeSet<&str>,
) -> Result<Vec<ToolRegistration>, ToolComponentError> {
    let mut selected = Vec::with_capacity(allowed.len());
    let mut remaining = allowed.clone();
    for declaration in catalog {
        if remaining.remove(declaration.descriptor.name().as_str()) {
            selected.push(ToolRegistration::new(declaration.descriptor, declaration.route));
        }
    }
    if let Some(name) = remaining.first() {
        return Err(ToolComponentError::new(
            ToolComponentErrorKind::UnknownTool,
            "construct configured tool inventory",
            format!("tool {name} has no production descriptor and dispatcher route"),
        ));
    }
    Ok(selected)
}

pub(super) fn operation_registry(
    registrations: &[ToolRegistration],
) -> Result<OperationRegistry, ToolComponentError> {
    let mut operations = Vec::with_capacity(registrations.len());
    for registration in registrations {
        let source = registration.descriptor().operation();
        let risks = RiskSet::new(source.risks().as_slice().to_vec()).map_err(operation_failure)?;
        operations.push(
            OperationDescriptor::new(source.name().clone(), source.operation_class(), risks)
                .map_err(operation_failure)?,
        );
    }
    OperationRegistry::new(operations).map_err(operation_failure)
}

fn operation_failure(error: PolicyError) -> ToolComponentError {
    ToolComponentError::new(
        ToolComponentErrorKind::OperationRegistry,
        "construct configured tool operations",
        error.code(),
    )
}
