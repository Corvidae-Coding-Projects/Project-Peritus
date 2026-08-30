//! Catalog and bundled asset coverage tests.

use std::collections::BTreeSet;

use peritus_security_qualification::{
    AcceptanceCriterion, H0_PRODUCTION_PROBE_COUNT, ProbeSpec, SecurityRequirement,
    bundled_security_assets,
};

#[test]
fn production_catalog_covers_every_literal_requirement_and_criterion() {
    let catalog = ProbeSpec::h0_production();
    assert_eq!(catalog.len(), H0_PRODUCTION_PROBE_COUNT);
    assert_eq!(
        catalog.iter().map(|probe| probe.requirement()).collect::<BTreeSet<_>>(),
        SecurityRequirement::ALL.into_iter().collect(),
    );
    assert_eq!(
        catalog.iter().map(|probe| probe.criterion()).collect::<BTreeSet<_>>(),
        AcceptanceCriterion::ALL.into_iter().collect(),
    );
    assert_eq!(
        catalog.iter().map(|probe| probe.id()).collect::<BTreeSet<_>>().len(),
        catalog.len(),
    );
}

#[test]
fn security_catalogs_and_schemas_are_packaged() {
    let assets = bundled_security_assets();
    assert_eq!(assets.len(), 11);
    assert!(assets.iter().all(|asset| !asset.contents().is_empty()));
    assert!(assets.iter().any(|asset| asset.path().ends_with("evidence-manifest-v1.schema.json")));
    assert!(
        assets.iter().any(|asset| asset.path().ends_with("native-probe-request-v1.schema.json"))
    );
    assert!(
        assets.iter().any(|asset| asset.path().ends_with("native-probe-response-v1.schema.json"))
    );
    assert!(assets.iter().any(|asset| asset.path().ends_with("native-shard-v1.schema.json")));
    assert!(assets.iter().any(|asset| asset.path().ends_with("native-candidate-v1.schema.json")));
    assert!(assets.iter().any(|asset| asset.path().ends_with("threat-model-v1.toml")));
}
