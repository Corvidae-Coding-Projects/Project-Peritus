//! Fixed provider fixtures shared by every production adapter bridge.

use super::{ProviderConformanceFixture, ProviderScenario};

pub(super) const fn fixture(scenario: ProviderScenario) -> ProviderConformanceFixture {
    ProviderConformanceFixture::new(scenario)
}
