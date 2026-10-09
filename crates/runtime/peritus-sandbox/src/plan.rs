//! Checked sandbox plan compilation.

use crate::{
    EnvironmentMode, FeatureSet, FileDecision, FileOperation, InputPermission,
    IsolationRequirement, NetworkDecision, ResizePermission, SandboxBinding, SandboxContract,
    SandboxError, SandboxFeature, SandboxOperationClass, SandboxRequirements, SecretDelivery,
    SignalPolicy, TerminalMode, TerminalSignalPermission, TreeContainment, canonical, error,
    verified,
};
use peritus_types::Sha256Digest;

/// Complete, canonical sandbox plan checked against all seven contract domains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedSandboxPlan {
    binding: SandboxBinding,
    isolation: IsolationRequirement,
    operation_class: SandboxOperationClass,
    contract: SandboxContract,
    requirements: SandboxRequirements,
    required_features: FeatureSet,
    canonical_bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl CheckedSandboxPlan {
    /// Returns the exact target binding.
    #[must_use]
    pub const fn binding(&self) -> SandboxBinding {
        self.binding
    }
    /// Returns requested isolation.
    #[must_use]
    pub const fn isolation(&self) -> IsolationRequirement {
        self.isolation
    }
    /// Returns the mapped operation class.
    #[must_use]
    pub const fn operation_class(&self) -> SandboxOperationClass {
        self.operation_class
    }
    /// Returns the checked source contract.
    #[must_use]
    pub const fn contract(&self) -> &SandboxContract {
        &self.contract
    }
    /// Returns checked invocation requirements.
    #[must_use]
    pub const fn requirements(&self) -> &SandboxRequirements {
        &self.requirements
    }
    /// Returns the complete backend feature requirement.
    #[must_use]
    pub const fn required_features(&self) -> FeatureSet {
        self.required_features
    }
    /// Returns the complete versioned canonical plan representation.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Returns the deterministic plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Compiles a complete sandbox plan or rejects the first denied domain.
///
/// # Errors
/// Returns a stable typed error for mismatched isolation/class or any requirement outside the
/// declared contract.
pub fn compile_sandbox(
    binding: SandboxBinding,
    isolation: IsolationRequirement,
    operation_class: SandboxOperationClass,
    contract: SandboxContract,
    requirements: SandboxRequirements,
) -> Result<CheckedSandboxPlan, SandboxError> {
    let facts = compilation_facts(isolation, operation_class, &contract, &requirements);
    let class_matches = (facts.isolation_ordinal == 0 && facts.operation_class_ordinal == 0)
        || (facts.isolation_ordinal == 1 && facts.operation_class_ordinal == 1);
    if !class_matches {
        return Err(error::denied("isolation and operation class disagree"));
    }
    if facts.filesystem_requested != facts.filesystem_admitted {
        return Err(error::denied("filesystem requirement is not allowed"));
    }
    if facts.process_requested != facts.process_admitted {
        return Err(error::denied("process requirement is not allowed"));
    }
    if facts.environment_requested != facts.environment_admitted {
        return Err(error::denied("environment requirement is not allowed"));
    }
    if facts.network_requested != facts.network_admitted {
        return Err(error::denied("network requirement is not allowed"));
    }
    if facts.secrets_requested != facts.secrets_admitted {
        return Err(error::denied("secret requirement is not allowed"));
    }
    if facts.resources_requested != facts.resources_admitted {
        return Err(error::denied("resource requirement exceeds contract"));
    }
    if facts.terminal_requested != facts.terminal_admitted {
        return Err(error::denied("terminal requirement is not allowed"));
    }
    if !verified::compilation_complete(facts) {
        return Err(error::denied("sandbox refinement projection is incomplete"));
    }
    let required_features = derive_features(&contract);
    let canonical_bytes = canonical::plan_bytes(
        binding,
        isolation,
        operation_class,
        &contract,
        &requirements,
        required_features,
    );
    let digest = peritus_codec::sha256(&canonical_bytes);
    Ok(CheckedSandboxPlan {
        binding,
        isolation,
        operation_class,
        contract,
        requirements,
        required_features,
        canonical_bytes,
        digest,
    })
}

fn compilation_facts(
    isolation: IsolationRequirement,
    operation_class: SandboxOperationClass,
    contract: &SandboxContract,
    requirements: &SandboxRequirements,
) -> verified::CompilationFacts {
    let filesystem_requested = requirements.files().len();
    let filesystem_admitted = requirements
        .files()
        .iter()
        .filter(|item| {
            contract.filesystem().decide(item.path(), item.operation()) == FileDecision::Allowed
        })
        .count();
    let process_admitted = usize::from(requirements.process().is_allowed_by(contract.process()));
    let inherited_requested = requirements.environment().inherited_names().len();
    let literal_requested = requirements.environment().literal_names().len();
    let environment_requested = inherited_requested + literal_requested;
    let environment_admitted = requirements
        .environment()
        .inherited_names()
        .iter()
        .filter(|name| contract.environment().permits_inherited(name))
        .count()
        + requirements
            .environment()
            .literal_names()
            .iter()
            .filter(|name| contract.environment().permits_literal(name))
            .count();
    let network_requested = requirements.network().len();
    let network_admitted = requirements
        .network()
        .iter()
        .filter(|target| contract.network().decide(target) == NetworkDecision::Allowed)
        .count();
    let secrets_requested = requirements.secrets().len();
    let secrets_admitted =
        requirements.secrets().iter().filter(|secret| contract.secrets().permits(secret)).count();
    let resources_admitted =
        usize::from(contract.resources().first_exceeded_by(requirements.resources()).is_none());
    let terminal_admitted = usize::from(requirements.terminal().is_allowed_by(contract.terminal()));
    verified::CompilationFacts {
        isolation_ordinal: isolation.ordinal(),
        operation_class_ordinal: operation_class.ordinal(),
        filesystem_requested,
        filesystem_admitted,
        process_requested: 1,
        process_admitted,
        environment_requested,
        environment_admitted,
        network_requested,
        network_admitted,
        secrets_requested,
        secrets_admitted,
        resources_requested: 1,
        resources_admitted,
        terminal_requested: 1,
        terminal_admitted,
    }
}

fn derive_features(contract: &SandboxContract) -> FeatureSet {
    let mut features = FeatureSet::empty();
    for operation in FileOperation::ALL {
        features.insert(operation.feature());
    }
    features.insert(SandboxFeature::ProcessRoot);
    features.insert(SandboxFeature::ProcessDescendants);
    if !matches!(contract.process().signals(), SignalPolicy::Denied) {
        features.insert(SandboxFeature::ProcessSignals);
    }
    if contract.process().containment() == TreeContainment::Required {
        features.insert(SandboxFeature::ProcessTree);
    }
    features.insert(match contract.environment().mode() {
        EnvironmentMode::Cleared => SandboxFeature::EnvironmentClear,
        EnvironmentMode::AllowListed(_) => SandboxFeature::EnvironmentAllowList,
    });
    features.insert(SandboxFeature::NetworkDeny);
    if !contract.network().rules().is_empty() {
        features.insert(SandboxFeature::NetworkEgress);
    }
    for grant in contract.secrets().grants() {
        features.insert(match grant.delivery() {
            SecretDelivery::Environment(_) => SandboxFeature::SecretEnvironment,
            SecretDelivery::File(_) => SandboxFeature::SecretFile,
            SecretDelivery::BrokeredHandle(_) => SandboxFeature::SecretHandle,
        });
    }
    for feature in [
        SandboxFeature::WallTime,
        SandboxFeature::CpuTime,
        SandboxFeature::Memory,
        SandboxFeature::Disk,
        SandboxFeature::Output,
        SandboxFeature::OpenHandles,
        SandboxFeature::ProcessCount,
        SandboxFeature::Concurrency,
    ] {
        features.insert(feature);
    }
    if contract.terminal().modes().contains(TerminalMode::Pipes) {
        features.insert(SandboxFeature::Pipes);
    }
    if contract.terminal().modes().contains(TerminalMode::Pty) {
        features.insert(SandboxFeature::Pty);
    }
    if contract.terminal().input() == InputPermission::Allowed {
        features.insert(SandboxFeature::Stdin);
    }
    if contract.terminal().resize() == ResizePermission::Allowed {
        features.insert(SandboxFeature::TerminalResize);
    }
    if contract.terminal().signals() == TerminalSignalPermission::Allowed {
        features.insert(SandboxFeature::TerminalSignals);
    }
    features
}
