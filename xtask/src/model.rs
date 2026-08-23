use serde::{Deserialize, Deserializer};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArchitecturePolicy {
    pub(crate) schema: u32,
    pub(crate) soft_source_lines: usize,
    pub(crate) hard_source_lines: usize,
    pub(crate) root_module_lines: usize,
    pub(crate) required_license: String,
    pub(crate) ignored_directories: Vec<String>,
    pub(crate) forbidden_module_names: Vec<String>,
    pub(crate) trusted_source_roots: Vec<PathBuf>,
    pub(crate) source_exceptions: Vec<SourceException>,
    pub(crate) layers: Vec<LayerPolicy>,
    pub(crate) verification_classes: Vec<VerificationClassPolicy>,
    pub(crate) forbidden_dependencies: Vec<ForbiddenDependencyPolicy>,
    pub(crate) controlled_source_roots: Vec<ControlledSourceRoot>,
    #[serde(default)]
    pub(crate) refinement_reservations: Vec<RefinementReservation>,
    pub(crate) packages: Vec<PackagePolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RefinementReservation {
    pub(crate) id: String,
    pub(crate) introduced_by: String,
    pub(crate) future_owner: String,
    pub(crate) statement: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayerPolicy {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) required_verification_class: Option<String>,
    pub(crate) may_depend_on: Vec<String>,
    pub(crate) may_dev_depend_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationClassPolicy {
    pub(crate) name: String,
    pub(crate) may_depend_on: Vec<String>,
    pub(crate) may_dev_depend_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForbiddenDependencyPolicy {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) rationale: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ControlledSourceKind {
    Generated,
    Schema,
    GeneratedSchema,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlledSourceRoot {
    pub(crate) path: PathBuf,
    pub(crate) owner: String,
    pub(crate) kind: ControlledSourceKind,
    pub(crate) rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackagePolicy {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) owner: String,
    pub(crate) layer: String,
    pub(crate) verification_class: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceException {
    pub(crate) path: PathBuf,
    pub(crate) owner: String,
    pub(crate) rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolchainPolicy {
    pub(crate) schema: u32,
    pub(crate) rust: String,
    pub(crate) verus: String,
    pub(crate) vstd_revision: String,
    pub(crate) z3: String,
    pub(crate) cargo_verus_advertised_z3: String,
    pub(crate) archives: ToolchainArchives,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolchainArchives {
    pub(crate) linux_x86_64: ToolchainArchive,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolchainArchive {
    pub(crate) url: String,
    pub(crate) sha256: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoMetadata {
    pub(crate) packages: Vec<CargoPackage>,
    pub(crate) workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoPackage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) edition: String,
    pub(crate) rust_version: Option<String>,
    pub(crate) license: Option<String>,
    pub(crate) manifest_path: PathBuf,
    pub(crate) readme: Option<PathBuf>,
    pub(crate) dependencies: Vec<CargoDependency>,
    pub(crate) targets: Vec<CargoTarget>,
    #[serde(default, deserialize_with = "deserialize_package_metadata")]
    pub(crate) metadata: CargoPackageMetadata,
}

fn deserialize_package_metadata<'de, D>(deserializer: D) -> Result<CargoPackageMetadata, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<CargoPackageMetadata>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoTarget {
    pub(crate) kind: Vec<String>,
    pub(crate) crate_types: Vec<String>,
    pub(crate) src_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoDependency {
    pub(crate) name: String,
    pub(crate) source: Option<String>,
    pub(crate) req: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) kind: Option<CargoDependencyKind>,
    pub(crate) target: Option<String>,
    pub(crate) optional: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(crate) enum CargoDependencyKind {
    #[serde(rename = "dev")]
    Development,
    #[serde(rename = "build")]
    Build,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct CargoPackageMetadata {
    pub(crate) peritus: Option<PeritusPackageMetadata>,
    pub(crate) verus: Option<VerusPackageMetadata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PeritusPackageMetadata {
    pub(crate) owner: String,
    pub(crate) layer: String,
    #[serde(rename = "verification-class")]
    pub(crate) verification_class: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct VerusPackageMetadata {
    #[serde(default)]
    pub(crate) verify: bool,
    pub(crate) no_vstd: Option<bool>,
    pub(crate) is_vstd: Option<bool>,
    pub(crate) is_core: Option<bool>,
    pub(crate) is_builtin: Option<bool>,
    pub(crate) is_builtin_macros: Option<bool>,
}

impl VerusPackageMetadata {
    pub(crate) const fn is_plain_verified(&self) -> bool {
        self.verify
            && self.no_vstd.is_none()
            && self.is_vstd.is_none()
            && self.is_core.is_none()
            && self.is_builtin.is_none()
            && self.is_builtin_macros.is_none()
    }
}
