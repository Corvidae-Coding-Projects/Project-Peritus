use peritus_conformance::{
    ConformanceSuite, PluginConformanceError, PluginConformanceFixture,
    PluginConformanceObservation, PluginConformanceSubject, plugin_suite,
};

struct CatalogSubject;

impl PluginConformanceSubject for CatalogSubject {
    fn exercise(
        &mut self,
        _fixture: &PluginConformanceFixture,
    ) -> Result<PluginConformanceObservation, PluginConformanceError> {
        Err(PluginConformanceError::Infrastructure)
    }
}

#[test]
fn plugin_catalog_suite_has_a_stable_nonempty_contract() {
    let suites = [plugin_suite::<CatalogSubject>()];
    let ids = suites.iter().map(|suite| suite.descriptor().id().as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["peritus.plugin"]);
    assert_eq!(suites[0].cases().len(), 7);
    assert!(
        suites[0].cases().windows(2).all(|pair| {
            pair[0].descriptor().id().as_str() < pair[1].descriptor().id().as_str()
        })
    );
}
