//! Exact acceptance-plan bindings derived from quality definitions.

use peritus_spec::{ContentReference, GateExecutionPlan, GateSuccessRule};
use peritus_types::{EnvironmentId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{
    CheckDefinition, CheckRequirement, CheckSource, ExpectedSuccess, OutputParser, QualityError,
    QualityErrorKind,
};

/// Content-addressed B2 bindings for one complete C4 quality definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QualityAcceptanceBinding {
    action: ContentReference,
    environment: EnvironmentId,
    inputs: ContentReference,
    parser: ContentReference,
    resources: ContentReference,
    success: GateSuccessRule,
    definition_digest: Sha256Digest,
}

impl QualityAcceptanceBinding {
    /// Returns the frozen action-plan reference.
    #[must_use]
    pub const fn action(self) -> ContentReference {
        self.action
    }

    /// Returns the exact execution environment.
    #[must_use]
    pub const fn environment(self) -> EnvironmentId {
        self.environment
    }

    /// Returns the frozen input-manifest reference.
    #[must_use]
    pub const fn inputs(self) -> ContentReference {
        self.inputs
    }

    /// Returns the frozen parser reference.
    #[must_use]
    pub const fn parser(self) -> ContentReference {
        self.parser
    }

    /// Returns the frozen resource declaration reference.
    #[must_use]
    pub const fn resources(self) -> ContentReference {
        self.resources
    }

    /// Returns the only success rule implemented by this exact definition.
    #[must_use]
    pub const fn success_rule(self) -> GateSuccessRule {
        self.success
    }

    /// Returns a digest covering every quality-definition field that affects execution or parsing.
    #[must_use]
    pub const fn definition_digest(self) -> Sha256Digest {
        self.definition_digest
    }

    /// Checks every frozen B2 execution-plan field exactly.
    #[must_use]
    pub fn matches(self, plan: GateExecutionPlan) -> bool {
        self.action == plan.action()
            && self.environment == plan.environment()
            && self.inputs == plan.inputs()
            && self.parser == plan.parser()
            && self.resources == plan.resources()
            && self.success == plan.success_rule()
    }
}

impl CheckDefinition {
    /// Derives the exact B2 content references implemented by this definition.
    ///
    /// The environment identity is supplied by the caller because the stable named C4 profile is
    /// resolved to a nominal B1/C2 environment at composition time. The existing plan compiler
    /// independently checks that resolved identity before execution.
    ///
    /// # Errors
    /// Returns a typed failure when the requested exit predicate is not representable by B2.
    pub fn acceptance_binding(
        &self,
        environment: EnvironmentId,
    ) -> Result<QualityAcceptanceBinding, QualityError> {
        let action = ContentReference::new(component_digest(b"action", |hash| {
            put_text(hash, self.executable());
            put_strings(hash, self.arguments());
            put_optional_text(hash, self.working_directory().map(ToString::to_string).as_deref());
        }));
        let inputs = ContentReference::new(component_digest(b"inputs", |hash| {
            hash.update(self.gate_id().as_bytes());
            put_source(hash, self.source());
            hash.update([requirement_tag(self.requirement())]);
            put_text(hash, self.environment_profile().as_str());
        }));
        let parser = ContentReference::new(parser_digest(self.parser()));
        let resources = ContentReference::new(component_digest(b"resources", |hash| {
            hash.update(self.timeout_millis().to_be_bytes());
            hash.update(self.output_bytes().to_be_bytes());
        }));
        let success = match (self.expected_success(), self.parser()) {
            (ExpectedSuccess::ExitCode(0), OutputParser::JsonSuccess { .. }) => {
                GateSuccessRule::Predicate(ContentReference::new(predicate_digest()))
            }
            (ExpectedSuccess::ExitCode(0), _) => GateSuccessRule::ExitCodeZero,
            (ExpectedSuccess::ExitCode(_), _) => {
                return Err(QualityError::new(
                    QualityErrorKind::InvalidInput,
                    "B2 gate plans can represent only zero-exit quality success",
                ));
            }
        };
        let definition_digest = component_digest(b"definition", |hash| {
            hash.update(action.digest().as_bytes());
            hash.update(environment.as_bytes());
            hash.update(inputs.digest().as_bytes());
            hash.update(parser.digest().as_bytes());
            hash.update(resources.digest().as_bytes());
            match success {
                GateSuccessRule::ExitCodeZero => hash.update([1]),
                GateSuccessRule::Predicate(reference) => {
                    hash.update([2]);
                    hash.update(reference.digest().as_bytes());
                }
            }
        });
        Ok(QualityAcceptanceBinding {
            action,
            environment,
            inputs,
            parser,
            resources,
            success,
            definition_digest,
        })
    }
}

fn component_digest(label: &[u8], append: impl FnOnce(&mut Sha256)) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-c4-quality-acceptance-v1\0");
    put_bytes(&mut hash, label);
    append(&mut hash);
    Sha256Digest::new(hash.finalize().into())
}

fn parser_digest(parser: OutputParser) -> Sha256Digest {
    component_digest(b"parser", |hash| match parser {
        OutputParser::None => hash.update([1]),
        OutputParser::Utf8 { maximum_bytes } => {
            hash.update([2]);
            hash.update(maximum_bytes.to_be_bytes());
        }
        OutputParser::Json { maximum_bytes } => {
            hash.update([3]);
            hash.update(maximum_bytes.to_be_bytes());
        }
        OutputParser::JsonSuccess { maximum_bytes } => {
            hash.update([4]);
            hash.update(maximum_bytes.to_be_bytes());
            hash.update(predicate_digest().as_bytes());
        }
    })
}

fn predicate_digest() -> Sha256Digest {
    component_digest(b"predicate", |hash| {
        put_text(hash, "exit-code-zero-and-json-object-boolean-success-equals-true");
    })
}

fn put_source(hash: &mut Sha256, source: &CheckSource) {
    match source {
        CheckSource::Explicit(label) => {
            hash.update([1]);
            put_text(hash, label);
        }
        CheckSource::CargoManifest => hash.update([2]),
        CheckSource::JustfileRecipe(recipe) => {
            hash.update([3]);
            put_text(hash, recipe);
        }
    }
}

const fn requirement_tag(requirement: CheckRequirement) -> u8 {
    match requirement {
        CheckRequirement::Required => 1,
        CheckRequirement::Optional => 2,
        CheckRequirement::Discovered => 3,
    }
}

fn put_optional_text(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            put_text(hash, value);
        }
        None => hash.update([0]),
    }
}

fn put_strings(hash: &mut Sha256, values: &[String]) {
    hash.update(u64::try_from(values.len()).unwrap_or(u64::MAX).to_be_bytes());
    for value in values {
        put_text(hash, value);
    }
}

fn put_text(hash: &mut Sha256, value: &str) {
    put_bytes(hash, value.as_bytes());
}

fn put_bytes(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

#[cfg(test)]
mod tests {
    use peritus_patch::WorkspacePath;
    use peritus_types::{EnvironmentId, GateId};

    use super::*;
    use crate::EnvironmentProfile;

    fn definition(parser: OutputParser) -> CheckDefinition {
        CheckDefinition::new(
            "gate.test",
            GateId::new([1; 16]).expect("gate"),
            CheckSource::Explicit("acceptance-v1".to_owned()),
            CheckRequirement::Required,
            "cargo",
            vec!["test".to_owned()],
            Some(WorkspacePath::new("crate").expect("path")),
            EnvironmentProfile::new("quality-default").expect("profile"),
            1_000,
            4_096,
            parser,
            ExpectedSuccess::ExitCode(0),
        )
        .expect("definition")
    }

    #[test]
    fn all_execution_relevant_fields_change_the_binding() {
        let environment = EnvironmentId::new([2; 16]).expect("environment");
        let plain = definition(OutputParser::Json { maximum_bytes: 64 })
            .acceptance_binding(environment)
            .expect("plain binding");
        let predicate = definition(OutputParser::JsonSuccess { maximum_bytes: 64 })
            .acceptance_binding(environment)
            .expect("predicate binding");
        assert_ne!(plain.definition_digest(), predicate.definition_digest());
        assert_eq!(plain.success_rule(), GateSuccessRule::ExitCodeZero);
        assert!(matches!(predicate.success_rule(), GateSuccessRule::Predicate(_)));
    }
}
