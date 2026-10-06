//! An unstarted provider owns its fixture until explicit observation and cleanup finish.

use peritus_conformance::{Observation, ProviderScenario};
use peritus_model_protocol::ProviderProfile;
use peritus_provider_openai::CodexRuntimeProvider;

use super::super::diagnostics::{ProbeError, boolean, unsigned};
use super::{FakeExecutable, profile, provider, read_trace};

pub(in super::super) struct ForeignProbe {
    helper: FakeExecutable,
    provider: CodexRuntimeProvider,
}

pub(in super::super) struct ForeignReceipt {
    pub requests: usize,
    trace_present: bool,
}

impl ForeignReceipt {
    pub fn evidence(&self) -> Vec<Observation> {
        vec![
            unsigned("foreign.requests", self.requests),
            boolean("foreign.trace-present", self.trace_present),
            boolean("foreign.directory-removed", true),
        ]
    }
}

impl ForeignProbe {
    pub fn untouched() -> Result<Self, ProbeError> {
        let profile = profile(ProviderScenario::AdapterIsolation, 0xD3)
            .map_err(|_| ProbeError::stage("foreign.profile"))?;
        Self::from_helper(FakeExecutable::install(ProviderScenario::AdapterIsolation)?, profile)
    }

    pub(super) fn from_helper(
        helper: FakeExecutable,
        profile: ProviderProfile,
    ) -> Result<Self, ProbeError> {
        match provider(helper.path(), profile) {
            Ok(provider) => Ok(Self { helper, provider }),
            Err(mut failure) => {
                failure.add_cleanup(&helper.directory.close());
                Err(failure)
            }
        }
    }

    pub fn finish(self) -> Result<ForeignReceipt, ProbeError> {
        let result = match read_trace(&self.helper.trace_path()) {
            Ok(trace) => Ok((trace.len(), true)),
            // This provider never starts: absence is evidence of zero requests only here.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((0, false)),
            Err(error) => Err(ProbeError::io("foreign.trace", &error)
                .with_evidence(vec![boolean("probe.trace-readable", false)])),
        };
        drop(self.provider);
        let cleanup = self.helper.directory.close();
        let mut failure = match result {
            Ok((requests, trace_present)) => match &cleanup {
                Ok(()) => return Ok(ForeignReceipt { requests, trace_present }),
                Err(error) => ProbeError::io("foreign.cleanup", error).with_evidence(vec![
                    unsigned("foreign.requests", requests),
                    boolean("foreign.trace-present", trace_present),
                ]),
            },
            Err(failure) => failure,
        };
        failure.add_cleanup(&cleanup);
        Err(failure)
    }
}
