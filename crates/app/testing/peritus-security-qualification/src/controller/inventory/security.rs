//! Exact threat-model and control-catalog reconciliation.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use crate::ProbeSpec;

use super::{ControllerError, SourceObservation, inventory_files, observation, parse_toml};

pub(super) fn threat(root: &Path) -> Result<SourceObservation, ControllerError> {
    let inventory: ThreatInventory = parse_toml(root, "security/threat-model-v1.toml")?;
    let expected_probes = catalog_ids();
    let mut probes = BTreeSet::new();
    let mut threats = BTreeSet::new();
    let mut assets = BTreeSet::new();
    for asset in &inventory.asset {
        if asset.boundary.is_empty() || !assets.insert(asset.id.as_str()) {
            return Err(ControllerError::protocol("threat asset inventory is incomplete"));
        }
    }
    for threat in &inventory.threat {
        if threat.title.is_empty()
            || threat.attack.is_empty()
            || threat.requirements.is_empty()
            || threat.probes.is_empty()
            || !threats.insert(threat.id.as_str())
            || threat.requirements.iter().any(|value| !requirement_ids().contains(&value.as_str()))
        {
            return Err(ControllerError::protocol("threat row is incomplete or duplicated"));
        }
        probes.extend(threat.probes.iter().map(String::as_str));
    }
    if inventory.schema != "peritus.security-threat-model.v1"
        || inventory.owner != "H0"
        || inventory.status != "qualification-input"
        || inventory.threat.len() != 10
        || probes != expected_probes
    {
        return Err(ControllerError::protocol("threat inventory does not cover the exact catalog"));
    }
    let files = inventory_files(root, &["security/threat-model-v1.toml"])?;
    Ok(observation("threat-inventory", &files, inventory.threat.len()))
}

pub(super) fn control(root: &Path) -> Result<SourceObservation, ControllerError> {
    let inventory: ControlInventory = parse_toml(root, "security/control-catalog-v1.toml")?;
    let expected_requirements = requirement_ids();
    let expected_criteria = [9_u8, 10, 11, 12, 17, 18, 19, 24, 25];
    let expected_probes = catalog_ids();
    let mut controls = BTreeSet::new();
    let mut covered_requirements = BTreeSet::new();
    let mut covered_criteria = BTreeSet::new();
    for control in &inventory.control {
        if control.title.is_empty()
            || control.requirements.is_empty()
            || control.criteria.is_empty()
            || !controls.insert(control.id.as_str())
        {
            return Err(ControllerError::protocol("control row is incomplete or duplicated"));
        }
        covered_requirements.extend(control.requirements.iter().map(String::as_str));
        covered_criteria.extend(control.criteria.iter().copied());
    }
    let probes = inventory.probe_catalog.ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if inventory.schema != "peritus.security-control-catalog.v1"
        || inventory.owner != "H0"
        || inventory.status != "qualification-input"
        || inventory.requirements.iter().map(String::as_str).collect::<Vec<_>>()
            != expected_requirements
        || inventory.acceptance_criteria != expected_criteria
        || inventory.control.len() != 10
        || covered_requirements != expected_requirements.into_iter().collect()
        || covered_criteria != expected_criteria.into_iter().collect()
        || probes != expected_probes
    {
        return Err(ControllerError::protocol(
            "control inventory does not cover exact obligations",
        ));
    }
    let files = inventory_files(root, &["security/control-catalog-v1.toml"])?;
    Ok(observation("control-inventory", &files, inventory.control.len()))
}

fn catalog_ids() -> BTreeSet<&'static str> {
    ProbeSpec::h0_production().iter().map(|spec| spec.id().as_str()).collect()
}

const fn requirement_ids() -> [&'static str; 7] {
    ["R-SEC-001", "R-SEC-002", "R-SEC-003", "R-SEC-004", "R-SEC-005", "R-SEC-006", "R-SEC-007"]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThreatInventory {
    schema: String,
    owner: String,
    status: String,
    asset: Vec<ThreatAsset>,
    threat: Vec<Threat>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThreatAsset {
    id: String,
    boundary: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Threat {
    id: String,
    title: String,
    attack: String,
    requirements: Vec<String>,
    probes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlInventory {
    schema: String,
    owner: String,
    status: String,
    requirements: Vec<String>,
    acceptance_criteria: Vec<u8>,
    control: Vec<Control>,
    probe_catalog: ProbeCatalog,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    id: String,
    title: String,
    requirements: Vec<String>,
    criteria: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeCatalog {
    ids: Vec<String>,
}
