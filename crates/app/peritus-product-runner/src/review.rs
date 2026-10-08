//! Typed independent-review parsing and conserved finding rendering.

use peritus_review::{
    FindingSeverity, ProductFinding, ProductFindingBodyFields, ProductFindingBodyPublisher,
    ProductFindingBodyReference, ProductFindingCategory, ProductFindingFieldReference,
    ProductFindingLedger, ProductFindingState, ProductReviewPage, ProductReviewSubmission,
    ProductReviewSummaryReference,
};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;

use crate::model_output::{TypedObjectError, last_typed_object};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewWire {
    summary: String,
    findings: Vec<FindingWire>,
    #[serde(default)]
    coverage_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindingWire {
    category: String,
    severity: String,
    title: String,
    description: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    reproduction: String,
    remediation: String,
}

pub fn parse(value: &str, cycle: u32) -> Result<ProductReviewSubmission, ProductRunnerError> {
    let review: ReviewWire = match last_typed_object(value) {
        Ok(review) => review,
        Err(TypedObjectError::Missing) => {
            return Err(invalid("review contains no JSON object"));
        }
        Err(TypedObjectError::Invalid(detail)) => {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "parse typed reviewer result",
                detail,
            ));
        }
    };
    let findings = review
        .findings
        .into_iter()
        .map(|finding| {
            let category = ProductFindingCategory::parse(&finding.category)
                .ok_or_else(|| invalid("review finding category is unknown"))?;
            let severity = severity(&finding.severity)
                .ok_or_else(|| invalid("review finding severity is unknown"))?;
            ProductFinding::new(
                category,
                severity,
                finding.title,
                finding.description,
                finding.location,
                finding.reproduction,
                finding.remediation,
                cycle,
            )
            .map_err(|error| invalid_owned(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    match review.coverage_cursor {
        Some(cursor) => ProductReviewSubmission::new_page(review.summary, findings, cursor),
        None => ProductReviewSubmission::new(review.summary, findings),
    }
    .map_err(|error| invalid_owned(error.to_string()))
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn render(ledger: &ProductFindingLedger) -> String {
    let summary_suffix = if ledger.review_summary_truncated() {
        " [preview truncated]"
    } else {
        ""
    };
    let mut text = format!(
        "Review cycle {}: {}{}\n",
        ledger.cycle(),
        ledger.review_summary(),
        summary_suffix,
    );
    if let Some(summary) = ledger.review_summary_reference() {
        text.push_str(&format!("{}\n", render_summary_locator(summary)));
    }
    if !ledger.pending_fixer().is_empty() {
        text.push_str("Admitted reviewer findings awaiting the next bounded fixer pass.\n");
        append_findings(
            &mut text,
            ledger,
            ledger.pending_fixer().iter().copied(),
        );
        return text;
    }
    let page = match ledger.review_page() {
        Ok(page) => page,
        Err(error) => {
            text.push_str(&format!("Finding index cannot advance: {error}.\n"));
            return text;
        }
    };
    text.push_str(&format!(
        "Bounded unresolved index page for review cycle {} (coverage cursor {}).\n",
        page.review_cycle(),
        page.coverage_cursor(),
    ));
    if page.entries().is_empty() {
        text.push_str("No unresolved findings in this index page.\n");
        return text;
    }
    append_findings(&mut text, ledger, page.entries().iter().map(|entry| entry.id()));
    text
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
fn append_findings(
    text: &mut String,
    ledger: &ProductFindingLedger,
    identities: impl Iterator<Item = Sha256Digest>,
) {
    for identity in identities {
        let Some(finding) = ledger.finding(identity) else { continue };
        let state = match finding.state() {
            ProductFindingState::Open => "open".to_owned(),
            ProductFindingState::FixProposed { cycle } => {
                format!("fix proposed in cycle {cycle}; reviewer confirmation pending")
            }
            ProductFindingState::ResolutionConfirmed { cycle } => {
                format!("resolution confirmed in cycle {cycle}")
            }
        };
        if let Some(body) = finding.body_reference() {
            text.push_str(&format!(
                "\n[{} / {:?} / {} / identity {}]\n{}\nProvenance: {}\n{}\n",
                finding.category().as_str(),
                finding.severity(),
                state,
                digest_hex(finding.id()),
                body.title_preview().text(),
                body.provenance_preview().text(),
                render_body_locator(body),
            ));
        } else {
            // Legacy embedders without a publication port retain the prior inline projection.
            text.push_str(&format!(
                "\n[{} / {:?} / {} / identity {}]\n{}\n{}\nLocation: {}\nReproduce: {}\nRemediation: {}\n",
                finding.category().as_str(),
                finding.severity(),
                state,
                digest_hex(finding.id()),
                finding.title(),
                finding.description(),
                finding.location(),
                finding.reproduction(),
                finding.remediation(),
            ));
        }
    }
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn render_reconciliation_page(page: &ProductReviewPage) -> String {
    let mut text = format!(
        "\n\nIndexed reconciliation page: review_cycle={} coverage_cursor={} entries={} next_page_wraps={}. Return this exact coverage_cursor to assert omission only for this page. Findings outside this page remain conserved automatically. The conserved finding source contains the exact fields; previews below are bounded catalog entries.\n",
        page.review_cycle(),
        page.coverage_cursor(),
        page.entries().len(),
        page.wraps_after_admission(),
    );
    for entry in page.entries() {
        let state = match entry.state() {
            ProductFindingState::Open => "open".to_owned(),
            ProductFindingState::FixProposed { cycle } => format!("fix_proposed:{cycle}"),
            ProductFindingState::ResolutionConfirmed { cycle } => {
                format!("resolution_confirmed:{cycle}")
            }
        };
        let title_suffix = if entry.title_truncated() { " [preview truncated]" } else { "" };
        let provenance_suffix =
            if entry.provenance_truncated() { " [preview truncated]" } else { "" };
        text.push_str(&format!(
            "identity={} category={} severity={:?} state={} title={}{} provenance={}{}{}\n",
            digest_hex(entry.id()),
            entry.category().as_str(),
            entry.severity(),
            state,
            entry.title(),
            title_suffix,
            entry.provenance(),
            provenance_suffix,
            entry.body_reference().map_or_else(String::new, |body| {
                format!(" {}", render_body_locator(body))
            }),
        ));
    }
    text
}

fn render_body_locator(body: &ProductFindingBodyReference) -> String {
    let fields = body.fields();
    format!(
        "finding_source={} sha256={} bytes={} fields=title:{}+{},description:{}+{},location:{}+{},provenance:{}+{},reproduction:{}+{},remediation:{}+{}",
        body.source_ordinal(),
        digest_hex(body.digest()),
        body.bytes(),
        fields.title().offset(),
        fields.title().bytes(),
        fields.description().offset(),
        fields.description().bytes(),
        fields.location().offset(),
        fields.location().bytes(),
        fields.provenance().offset(),
        fields.provenance().bytes(),
        fields.reproduction().offset(),
        fields.reproduction().bytes(),
        fields.remediation().offset(),
        fields.remediation().bytes(),
    )
}

fn render_summary_locator(summary: &ProductReviewSummaryReference) -> String {
    let field = summary.summary();
    format!(
        "review_summary_source={} sha256={} bytes={} field=summary:{}+{}",
        summary.source_ordinal(),
        digest_hex(summary.digest()),
        summary.bytes(),
        field.offset(),
        field.bytes(),
    )
}

pub fn encode_ledger(ledger: &ProductFindingLedger) -> Result<String, ProductRunnerError> {
    let findings = ledger
        .findings()
        .map(|finding| {
            let (state, state_cycle) = match finding.state() {
                ProductFindingState::Open => ("open", 0),
                ProductFindingState::FixProposed { cycle } => ("fix_proposed", cycle),
                ProductFindingState::ResolutionConfirmed { cycle } => {
                    ("resolution_confirmed", cycle)
                }
            };
            let (provenance, title, description, location, reproduction, remediation, body) =
                if let Some(reference) = finding.body_reference() {
                    (None, None, None, None, None, None, Some(body_wire(reference)))
                } else {
                    (
                        Some(finding.provenance().to_owned()),
                        Some(finding.title().to_owned()),
                        Some(finding.description().to_owned()),
                        Some(finding.location().to_owned()),
                        Some(finding.reproduction().to_owned()),
                        Some(finding.remediation().to_owned()),
                        None,
                    )
                };
            DurableFindingWire {
                identity: Some(finding.id().into_bytes()),
                identity_version: Some(finding.identity_version()),
                provenance,
                category: finding.category().as_str().to_owned(),
                severity: severity_text(finding.severity()).to_owned(),
                title,
                description,
                location,
                reproduction,
                remediation,
                body,
                state: state.to_owned(),
                state_cycle,
                first_cycle: finding.first_cycle(),
                last_cycle: finding.last_cycle(),
            }
        })
        .collect();
    serde_json::to_string(&DurableLedgerWire {
        format_version: CURRENT_LEDGER_FORMAT,
        cycle: ledger.cycle(),
        summary: ledger.inline_review_summary().map(str::to_owned),
        summary_body: ledger.review_summary_reference().map(summary_wire),
        coverage_after: ledger.coverage_after().map(Sha256Digest::into_bytes),
        pending_fixer: ledger
            .pending_fixer()
            .iter()
            .copied()
            .map(Sha256Digest::into_bytes)
            .collect(),
        findings,
    })
    .map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "encode durable D2 finding ledger",
            error.to_string(),
        )
    })
}

pub fn restore_ledger(value: &str) -> Result<ProductFindingLedger, ProductRunnerError> {
    if value.is_empty() {
        return Ok(ProductFindingLedger::new());
    }
    let wire: DurableLedgerWire = serde_json::from_str(value).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "decode durable D2 finding ledger",
            error.to_string(),
        )
    })?;
    if !matches!(wire.format_version, 0 | PREVIOUS_LEDGER_FORMAT | CURRENT_LEDGER_FORMAT) {
        return Err(invalid("durable finding ledger version is unsupported"));
    }
    let version = wire.format_version;
    let findings = wire
        .findings
        .into_iter()
        .map(|finding| restore_finding(finding, version))
        .collect::<Result<Vec<_>, _>>()?;
    let coverage_after = wire.coverage_after.map(Sha256Digest::new);
    let pending_fixer = wire.pending_fixer.into_iter().map(Sha256Digest::new).collect();
    let restored = match (version, wire.summary, wire.summary_body) {
        (0 | PREVIOUS_LEDGER_FORMAT, Some(summary), None)
        | (CURRENT_LEDGER_FORMAT, Some(summary), None) => ProductFindingLedger::restore_durable(
            wire.cycle,
            summary,
            coverage_after,
            pending_fixer,
            findings,
        ),
        (CURRENT_LEDGER_FORMAT, None, Some(summary)) => {
            ProductFindingLedger::restore_durable_stored(
                wire.cycle,
                restore_summary_reference(wire.cycle, summary)?,
                coverage_after,
                pending_fixer,
                findings,
            )
        }
        _ => return Err(invalid("durable finding ledger summary authority is invalid")),
    };
    restored
    .map_err(|error| invalid_owned(error.to_string()))
}

impl crate::ProductRunner {
    /// Decodes the compact authoritative finding ledger for a daemon-owned source catalog.
    ///
    /// # Errors
    /// Rejects malformed, unsupported, or internally inconsistent durable state.
    pub fn decode_finding_state(
        value: &str,
    ) -> Result<ProductFindingLedger, ProductRunnerError> {
        restore_ledger(value)
    }

    /// Publishes legacy inline bodies and returns the compact ledger projection for atomic adoption.
    ///
    /// # Errors
    /// Rejects invalid durable state, publication failure, or compact encoding failure.
    pub fn externalize_finding_state(
        value: &str,
        publisher: &dyn ProductFindingBodyPublisher,
    ) -> Result<(String, bool), ProductRunnerError> {
        let mut ledger = restore_ledger(value)?;
        let changed = ledger.externalize_bodies(publisher).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "publish durable D2 finding body",
                error.to_string(),
            )
        })?;
        encode_ledger(&ledger).map(|encoded| (encoded, changed))
    }
}

const PREVIOUS_LEDGER_FORMAT: u16 = 2;
const CURRENT_LEDGER_FORMAT: u16 = 3;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableLedgerWire {
    #[serde(default)]
    format_version: u16,
    cycle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary_body: Option<DurableSummaryWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coverage_after: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_fixer: Vec<[u8; 32]>,
    findings: Vec<DurableFindingWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity_version: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<String>,
    category: String,
    severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reproduction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remediation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<DurableBodyWire>,
    state: String,
    state_cycle: u32,
    first_cycle: u32,
    last_cycle: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableBodyWire {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    fields: DurableBodyFieldsWire,
    title_preview: DurablePreviewWire,
    provenance_preview: DurablePreviewWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableSummaryWire {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    summary: DurableFieldWire,
    preview: DurablePreviewWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableBodyFieldsWire {
    title: DurableFieldWire,
    description: DurableFieldWire,
    location: DurableFieldWire,
    provenance: DurableFieldWire,
    reproduction: DurableFieldWire,
    remediation: DurableFieldWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFieldWire {
    offset: u64,
    bytes: u64,
    digest: [u8; 32],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurablePreviewWire {
    text: String,
    truncated: bool,
    binding: [u8; 32],
}

fn restore_finding(
    finding: DurableFindingWire,
    ledger_version: u16,
) -> Result<ProductFinding, ProductRunnerError> {
    let category = ProductFindingCategory::parse(&finding.category)
        .ok_or_else(|| invalid("durable finding category is unknown"))?;
    let severity = severity(&finding.severity)
        .ok_or_else(|| invalid("durable finding severity is unknown"))?;
    let state = match finding.state.as_str() {
        "open" if finding.state_cycle == 0 => ProductFindingState::Open,
        "fix_proposed" => ProductFindingState::FixProposed { cycle: finding.state_cycle },
        "resolution_confirmed" => {
            ProductFindingState::ResolutionConfirmed { cycle: finding.state_cycle }
        }
        _ => return Err(invalid("durable finding state is invalid")),
    };
    if let Some(body) = finding.body {
        if !matches!(ledger_version, PREVIOUS_LEDGER_FORMAT | CURRENT_LEDGER_FORMAT)
            || finding.provenance.is_some()
            || finding.title.is_some()
            || finding.description.is_some()
            || finding.location.is_some()
            || finding.reproduction.is_some()
            || finding.remediation.is_some()
        {
            return Err(invalid("compact durable finding contains inline body fields"));
        }
        let identity = finding
            .identity
            .ok_or_else(|| invalid("compact durable finding identity is missing"))?;
        let identity_version = finding
            .identity_version
            .ok_or_else(|| invalid("compact durable finding identity version is missing"))?;
        let reference = restore_body_reference(
            Sha256Digest::new(identity),
            identity_version,
            body,
        )?;
        return ProductFinding::restore_stored(
            Sha256Digest::new(identity),
            identity_version,
            category,
            severity,
            reference,
            state,
            finding.first_cycle,
            finding.last_cycle,
        )
        .map_err(|error| invalid_owned(error.to_string()));
    }
    let title = finding.title.ok_or_else(|| invalid("durable finding title is missing"))?;
    let description = finding
        .description
        .ok_or_else(|| invalid("durable finding description is missing"))?;
    let location = finding
        .location
        .ok_or_else(|| invalid("durable finding location is missing"))?;
    let reproduction = finding
        .reproduction
        .ok_or_else(|| invalid("durable finding reproduction is missing"))?;
    let remediation = finding
        .remediation
        .ok_or_else(|| invalid("durable finding remediation is missing"))?;
    let restored = match (finding.identity, finding.identity_version, finding.provenance) {
        (None, None, None) => ProductFinding::restore(
            category,
            severity,
            title,
            description,
            location,
            reproduction,
            remediation,
            state,
            finding.first_cycle,
            finding.last_cycle,
        ),
        (Some(identity), Some(identity_version), Some(provenance)) => {
            ProductFinding::restore_indexed(
                Sha256Digest::new(identity),
                identity_version,
                provenance,
                category,
                severity,
                title,
                description,
                location,
                reproduction,
                remediation,
                state,
                finding.first_cycle,
                finding.last_cycle,
            )
        }
        _ => return Err(invalid("durable finding identity is incomplete")),
    };
    restored.map_err(|error| invalid_owned(error.to_string()))
}

fn body_wire(reference: &ProductFindingBodyReference) -> DurableBodyWire {
    let fields = reference.fields();
    DurableBodyWire {
        digest: reference.digest().into_bytes(),
        bytes: reference.bytes(),
        source_ordinal: reference.source_ordinal(),
        fields: DurableBodyFieldsWire {
            title: field_wire(fields.title()),
            description: field_wire(fields.description()),
            location: field_wire(fields.location()),
            provenance: field_wire(fields.provenance()),
            reproduction: field_wire(fields.reproduction()),
            remediation: field_wire(fields.remediation()),
        },
        title_preview: DurablePreviewWire {
            text: reference.title_preview().text().to_owned(),
            truncated: reference.title_preview().truncated(),
            binding: reference.title_preview().binding().into_bytes(),
        },
        provenance_preview: DurablePreviewWire {
            text: reference.provenance_preview().text().to_owned(),
            truncated: reference.provenance_preview().truncated(),
            binding: reference.provenance_preview().binding().into_bytes(),
        },
    }
}

fn summary_wire(reference: &ProductReviewSummaryReference) -> DurableSummaryWire {
    DurableSummaryWire {
        digest: reference.digest().into_bytes(),
        bytes: reference.bytes(),
        source_ordinal: reference.source_ordinal(),
        summary: field_wire(reference.summary()),
        preview: DurablePreviewWire {
            text: reference.preview().text().to_owned(),
            truncated: reference.preview().truncated(),
            binding: reference.preview().binding().into_bytes(),
        },
    }
}

const fn field_wire(field: ProductFindingFieldReference) -> DurableFieldWire {
    DurableFieldWire {
        offset: field.offset(),
        bytes: field.bytes(),
        digest: field.digest().into_bytes(),
    }
}

fn restore_body_reference(
    finding_id: Sha256Digest,
    identity_version: u8,
    body: DurableBodyWire,
) -> Result<ProductFindingBodyReference, ProductRunnerError> {
    let fields = ProductFindingBodyFields::restore(
        restore_field(body.fields.title),
        restore_field(body.fields.description),
        restore_field(body.fields.location),
        restore_field(body.fields.provenance),
        restore_field(body.fields.reproduction),
        restore_field(body.fields.remediation),
    )
    .map_err(|error| invalid_owned(error.to_string()))?;
    ProductFindingBodyReference::restore(
        finding_id,
        identity_version,
        Sha256Digest::new(body.digest),
        body.bytes,
        body.source_ordinal,
        fields,
        body.title_preview.text,
        body.title_preview.truncated,
        Sha256Digest::new(body.title_preview.binding),
        body.provenance_preview.text,
        body.provenance_preview.truncated,
        Sha256Digest::new(body.provenance_preview.binding),
    )
    .map_err(|error| invalid_owned(error.to_string()))
}

fn restore_summary_reference(
    review_cycle: u32,
    summary: DurableSummaryWire,
) -> Result<ProductReviewSummaryReference, ProductRunnerError> {
    ProductReviewSummaryReference::restore(
        review_cycle,
        Sha256Digest::new(summary.digest),
        summary.bytes,
        summary.source_ordinal,
        restore_field(summary.summary),
        summary.preview.text,
        summary.preview.truncated,
        Sha256Digest::new(summary.preview.binding),
    )
    .map_err(|error| invalid_owned(error.to_string()))
}

const fn restore_field(field: DurableFieldWire) -> ProductFindingFieldReference {
    ProductFindingFieldReference::new(
        field.offset,
        field.bytes,
        Sha256Digest::new(field.digest),
    )
}

fn severity(value: &str) -> Option<FindingSeverity> {
    match value {
        "advisory" => Some(FindingSeverity::Advisory),
        "low" => Some(FindingSeverity::Low),
        "medium" => Some(FindingSeverity::Medium),
        "high" => Some(FindingSeverity::High),
        "critical" => Some(FindingSeverity::Critical),
        _ => None,
    }
}

fn digest_hex(value: Sha256Digest) -> String {
    let mut output = String::with_capacity(Sha256Digest::LENGTH.saturating_mul(2));
    for byte in value.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn severity_text(value: FindingSeverity) -> &'static str {
    match value {
        FindingSeverity::Advisory => "advisory",
        FindingSeverity::Low => "low",
        FindingSeverity::Medium => "medium",
        FindingSeverity::High => "high",
        FindingSeverity::Critical => "critical",
    }
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate typed reviewer result",
        detail,
    )
}

fn invalid_owned(detail: String) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate typed reviewer result",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_selects_final_typed_object_after_braced_evidence() {
        let response = r#"Observed output {'key': 'value'}.
{"summary":"Layer 2 fixed; gates pass.","findings":[]}"#;

        let submission = parse(response, 1).expect("typed review");

        assert_eq!(submission.summary(), "Layer 2 fixed; gates pass.");
        assert!(submission.findings().is_empty());
    }

    #[test]
    fn reviewer_boolean_is_rejected_and_advisory_remains_nonblocking() {
        assert!(parse(r#"{"summary":"looks fine","blocking":false,"findings":[]}"#, 1).is_err());
        assert!(parse(r#"{"summary":"still inspecting"}"#, 1).is_err());
        let submission = parse(
            r#"{"summary":"target was missed","findings":[{"category":"build_coverage","severity":"advisory","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            1,
        )
        .expect("typed review");
        let mut ledger = ProductFindingLedger::new();
        ledger.admit_review(1, submission).expect("admit");
        assert!(!ledger.has_blockers());

        let submission = parse(
            r#"{"summary":"target remains uncovered","findings":[{"category":"build_coverage","severity":"low","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            2,
        )
        .expect("typed review");
        ledger.admit_review(2, submission).expect("admit");
        assert!(ledger.has_blockers());
    }

    #[test]
    fn durable_ledger_round_trip_preserves_a_fixer_proposal() {
        let submission = parse(
            r#"{"summary":"target was missed","findings":[{"category":"build_coverage","severity":"high","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            1,
        )
        .expect("typed review");
        let mut ledger = ProductFindingLedger::new();
        ledger.admit_review(1, submission).expect("admit");
        ledger.record_fixer_proposal(1);
        let restored = restore_ledger(&encode_ledger(&ledger).expect("encode")).expect("restore");
        assert!(matches!(
            restored.open_findings().next().expect("finding").state(),
            ProductFindingState::FixProposed { cycle: 1 },
        ));
    }

    #[test]
    fn durable_clean_review_round_trip_can_continue_with_the_next_review() {
        let mut ledger = ProductFindingLedger::new();
        for cycle in 1..=2 {
            let submission = parse(r#"{"summary":"No candidate defects","findings":[]}"#, cycle)
                .expect("clean review");
            ledger.admit_review(cycle, submission).expect("admit next review");
            let restored = restore_ledger(&encode_ledger(&ledger).expect("encode"))
                .expect("restore clean review");
            assert_eq!(restored, ledger);
            ledger = restored;
        }
    }
}
