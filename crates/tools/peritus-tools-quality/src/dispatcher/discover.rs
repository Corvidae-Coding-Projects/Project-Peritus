//! Authorized immutable quality surface discovery.

use core::fmt::Write;

use peritus_tool_protocol::{
    BoundedJson, ImplementationIdentity, JsonLimits, SchemaDigest, ToolResult, ToolTiming,
    Truncation, TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_workspace::ReadOnlyWorkspace;

use super::adapter_failure;
use crate::{
    CheckCatalog, CheckDefinition, CheckRequirement, CheckSource, OutputParser,
    discover_descriptor, discovery::inspect, json_value::object, render::text,
};

/// One-use inspection dispatcher over an immutable C1 workspace snapshot.
pub struct QualityDiscoverDispatcher<'workspace> {
    workspace: &'workspace ReadOnlyWorkspace,
    explicit: Option<Vec<CheckDefinition>>,
    descriptor: peritus_tool_protocol::ToolDescriptor,
    catalog: Option<CheckCatalog>,
}

impl<'workspace> QualityDiscoverDispatcher<'workspace> {
    /// Binds an immutable C1 workspace and explicit typed project/B2 definitions.
    ///
    /// # Errors
    /// Returns a typed failure only if canonical descriptor construction fails.
    pub fn new(
        workspace: &'workspace ReadOnlyWorkspace,
        explicit: Vec<CheckDefinition>,
    ) -> Result<Self, crate::QualityError> {
        Ok(Self {
            workspace,
            explicit: Some(explicit),
            descriptor: discover_descriptor()?,
            catalog: None,
        })
    }

    /// Borrows the combined catalog after an authorized successful dispatch.
    #[must_use]
    pub const fn catalog(&self) -> Option<&CheckCatalog> {
        self.catalog.as_ref()
    }
}

impl ToolDispatcher for QualityDiscoverDispatcher<'_> {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        self.descriptor.implementation_identity()
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        if invocation.prepared().descriptor_digest() != self.descriptor.descriptor_digest()
            || invocation.prepared().descriptor().name().as_str() != "quality.discover"
        {
            return Err(adapter_failure(
                "quality-discover-invocation",
                "authorized invocation differs from the discovery descriptor",
            ));
        }
        let revision = invocation.binding().revision();
        let permit = invocation.binding();
        let snapshot = self.workspace.snapshot();
        let target_matches = self.workspace.target_binding().is_some_and(|target| {
            target.workspace_id() == permit.revision().workspace_id()
                && target.environment_id() == permit.environment_id()
                && target.resource_id() == permit.resource_id()
        });
        if revision != invocation.prepared().call().revision()
            || revision.workspace_id() != snapshot.workspace_id()
            || revision.workspace_generation() != snapshot.generation()
            || revision.workspace_revision() != snapshot.revision()
            || !target_matches
        {
            return Err(adapter_failure(
                "quality-discover-target",
                "authorized permit target differs from the immutable C1 snapshot",
            ));
        }
        let observed_at = invocation.observed_at();
        let prepared = invocation.into_prepared();
        let explicit = self.explicit.take().ok_or_else(|| {
            adapter_failure(
                "quality-discover-consumed",
                "discovery dispatcher was already consumed",
            )
        })?;
        let catalog = inspect(self.workspace, explicit)
            .map_err(|error| adapter_failure("quality-discovery", error.detail()))?;
        let structured = catalog_json(&catalog)
            .map_err(|error| adapter_failure("quality-discovery-result", &error.to_string()))?;
        let count = catalog.checks().len();
        let summary = format!("discovered {count} explicit or known quality checks");
        let timing = ToolTiming::new(observed_at, observed_at)
            .map_err(|error| adapter_failure("quality-discovery-timing", &error.to_string()))?;
        let result = ToolResult::success(
            &prepared,
            structured,
            text(summary.clone()),
            text(summary),
            Vec::new(),
            timing,
            TruncationMetadata {
                output: Truncation::Complete,
                model: Truncation::Complete,
                human: Truncation::Complete,
            },
            0,
        )
        .map_err(|error| adapter_failure("quality-discovery-result", &error.to_string()))?;
        self.catalog = Some(catalog);
        Ok(ToolStart::Completed(result))
    }
}

fn catalog_json(
    catalog: &CheckCatalog,
) -> Result<BoundedJson, peritus_tool_protocol::ProtocolError> {
    let checks: Vec<_> = catalog
        .checks()
        .iter()
        .map(|entry| {
            let check = entry.definition();
            object([
                (
                    "arguments",
                    serde_json::Value::Array(
                        check.arguments().iter().cloned().map(serde_json::Value::String).collect(),
                    ),
                ),
                (
                    "environment_profile",
                    serde_json::Value::String(check.environment_profile().as_str().to_owned()),
                ),
                ("executable", serde_json::Value::String(check.executable().to_owned())),
                (
                    "expected_success",
                    serde_json::Value::String(expected_success_name(check.expected_success())),
                ),
                ("gate_id", serde_json::Value::String(hex(check.gate_id().as_bytes()))),
                ("gate_name", serde_json::Value::String(check.gate_name().to_owned())),
                ("output_bytes", serde_json::Value::String(check.output_bytes().to_string())),
                ("parser", serde_json::Value::String(parser_name(check.parser()).to_owned())),
                (
                    "requirement",
                    serde_json::Value::String(requirement_name(check.requirement()).to_owned()),
                ),
                ("source", serde_json::Value::String(source_name(check.source()))),
                ("timeout_millis", serde_json::Value::String(check.timeout_millis().to_string())),
                (
                    "working_directory",
                    check.working_directory().map_or(serde_json::Value::Null, |path| {
                        serde_json::Value::String(path.to_string())
                    }),
                ),
            ])
        })
        .collect();
    let value = object([("checks", serde_json::Value::Array(checks))]);
    BoundedJson::parse(&value.to_string(), JsonLimits::PRODUCTION)
}

const fn requirement_name(value: CheckRequirement) -> &'static str {
    match value {
        CheckRequirement::Required => "required",
        CheckRequirement::Optional => "optional",
        CheckRequirement::Discovered => "discovered",
    }
}

fn source_name(value: &CheckSource) -> String {
    match value {
        CheckSource::Explicit(label) => format!("explicit:{label}"),
        CheckSource::CargoManifest => "cargo-manifest".to_owned(),
        CheckSource::JustfileRecipe(recipe) => format!("justfile:{recipe}"),
    }
}

const fn parser_name(value: OutputParser) -> &'static str {
    match value {
        OutputParser::None => "none",
        OutputParser::Utf8 { .. } => "utf8",
        OutputParser::Json { .. } => "json",
    }
}

fn expected_success_name(value: crate::ExpectedSuccess) -> String {
    match value {
        crate::ExpectedSuccess::ExitCode(code) => format!("exit-code:{code}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
        write!(value, "{byte:02x}").expect("writing to a string cannot fail");
        value
    })
}
