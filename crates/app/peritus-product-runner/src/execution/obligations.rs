//! Literal public obligations carried through gates, review, acceptance, and fixer routing.

use core::fmt::Write as _;

use peritus_obligations::{
    DirectEvidence, EvidenceBinding, FailureContext, FailureDisposition, FailureOwner,
    ObligationLimits, ObligationSpec, PublicTaskSource, QualificationReport as LegacyQualification,
    RequirementDraft, RequirementEvidence, RequirementLedger, SourceObligationLedgerRoot,
    SourceObligationPage, SourceObligationQualification,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{ProductRunInput, check_cancelled, obligation_store::ObligationStore};
use crate::{
    ContextSource, ContextSourceKind, ProductRunnerError, ProductRunnerErrorKind, bundle,
};

// This fixed V2 framing capacity is part of the page digest, not a lifetime conversation limit.
const SOURCE_PAGE_BYTES: usize = 32 * 1024;
const PROJECTION_BODY_BYTES: usize = 240 * 1024;

/// Exact public requirement ledger used by every qualifying phase.
pub(super) struct RunObligations {
    ledger: ObligationLedger,
}

enum ObligationLedger {
    Legacy(RequirementLedger),
    SourceBacked { root: SourceObligationLedgerRoot, projection: String },
}

pub(super) enum RunQualification {
    Legacy(LegacyQualification),
    SourceBacked(SourceObligationQualification),
}

impl RunQualification {
    pub(super) const fn qualified(&self) -> bool {
        match self {
            Self::Legacy(report) => report.qualified(),
            Self::SourceBacked(report) => report.qualified(),
        }
    }
}

/// Current acceptance conclusions for one exact candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct QualificationState {
    gates: bool,
    obligations: bool,
    review: bool,
}

impl QualificationState {
    pub(super) const fn new(gates: bool, obligations: bool, review: bool) -> Self {
        Self { gates, obligations, review }
    }

    pub(super) const fn all_satisfied(self) -> bool {
        self.gates && self.obligations && self.review
    }

    pub(super) const fn fixer_disposition(self, recovery_available: bool) -> FailureDisposition {
        let owner = if self.all_satisfied() {
            FailureOwner::HarnessInfrastructure
        } else {
            FailureOwner::CandidateDefect
        };
        owner.disposition(FailureContext::new(false, recovery_available))
    }
}

impl RunObligations {
    #[allow(
        dead_code,
        reason = "the V1 constructor preserves retained ledger compatibility while production publishes V2"
    )]
    pub(super) fn capture(
        transcript: &str,
        conversation_revision: u64,
    ) -> Result<Self, ProductRunnerError> {
        let limits = ObligationLimits::production();
        let source =
            PublicTaskSource::new(transcript.as_bytes().to_vec(), conversation_revision, limits)
                .map_err(invalid)?;
        let spans = clause_spans(transcript, limits.max_clause_bytes());
        let mut drafts = spans
            .into_iter()
            .map(|(start, end)| {
                let id = occurrence_id(
                    &source,
                    start,
                    end,
                    &transcript.as_bytes()[start..end],
                )?;
                Ok(RequirementDraft::new(id, start, end, ObligationSpec::Hard, Vec::new()))
            })
            .collect::<Result<Vec<_>, ProductRunnerError>>()?;
        drafts.sort_by_key(RequirementDraft::id);
        let ledger = RequirementLedger::extract(&source, drafts, limits).map_err(invalid)?;
        Ok(Self { ledger: ObligationLedger::Legacy(ledger) })
    }

    /// Captures the complete governing conversation without imposing a lifetime byte or source
    /// count ceiling. Governed hosts retain bodies in their durable source store; legacy hosts
    /// receive the same V2 page contract over their complete rendered transcript.
    pub(super) fn capture_input(input: &ProductRunInput) -> Result<Self, ProductRunnerError> {
        let revision = input
            .conversation
            .request_source_revision()
            .map_err(|detail| host("read authoritative request-source revision", detail))?;
        let source_binding = Sha256Digest::new(input.conversation.request_source_binding());
        let catalog_binding =
            Sha256Digest::new(input.conversation.request_source_catalog_binding());
        let mut store_path = input.trace_path.clone().into_os_string();
        store_path.push(".obligations.sqlite3");
        let store_path = std::path::PathBuf::from(store_path);
        let mut store = ObligationStore::open(&store_path, input)?;
        let retained = store.load_root(input, catalog_binding)?;
        let mut capture = SourceCapture::new(revision);
        let required = input
            .conversation
            .request_sources_required()
            .map_err(|detail| host("inspect authoritative request-source policy", detail))?;
        match input.conversation.request_sources(None) {
            Ok(first) => capture_catalog(input, &mut store, &mut capture, first)?,
            Err(detail) if required => {
                return Err(host("read authoritative request-source catalog", detail));
            }
            Err(_) => capture_inline(input, &mut store, &mut capture)?,
        }
        if input.conversation.revision() != revision
            || input.conversation.request_source_revision().map_err(|detail| {
                host("recheck authoritative request-source revision", detail)
            })? != revision
            || input.conversation.request_source_binding() != *source_binding.as_bytes()
            || input.conversation.request_source_catalog_binding() != *catalog_binding.as_bytes()
        {
            return Err(host(
                "publish authoritative obligation ledger",
                "the governing conversation changed while its obligation pages were captured",
            ));
        }
        let root = SourceObligationLedgerRoot::new(
            source_binding,
            catalog_binding,
            revision,
            capture.page_count,
            capture.previous_page,
        )
        .map_err(invalid)?;
        let newly_published = match retained {
            Some(retained) if retained == root => false,
            Some(_) => {
                return Err(invariant(
                    "the current request-source catalog conflicts with its retained obligation root",
                ));
            }
            None => {
                // Whether this insert wins a concurrent exact race or reconciles an ambiguous
                // commit, this invocation must adopt the V2 contract before reusing evidence.
                let _ = store.publish_root(input, root)?;
                true
            }
        };
        capture.finish_projection(root, newly_published);
        Ok(Self {
            ledger: ObligationLedger::SourceBacked { root, projection: capture.projection },
        })
    }

    pub(super) fn qualify(
        &self,
        candidate: &CandidateIdentity,
        gates_satisfied: bool,
        review_satisfied: bool,
        evidence_text: &str,
    ) -> Result<RunQualification, ProductRunnerError> {
        let satisfied = gates_satisfied && review_satisfied;
        match &self.ledger {
            ObligationLedger::Legacy(ledger) => {
                let evidence_digest =
                    digest(b"peritus-product-obligation-evidence-v1", evidence_text.as_bytes());
                let mut evidence = ledger
                    .entries()
                    .iter()
                    .map(|entry| {
                        let binding = EvidenceBinding::new(
                            entry.id(),
                            ledger.digest(),
                            *candidate,
                            evidence_digest,
                            Vec::new(),
                            ledger.limits(),
                        )
                        .map_err(invalid)?;
                        Ok(RequirementEvidence::Direct(DirectEvidence::new(binding, satisfied)))
                    })
                    .collect::<Result<Vec<_>, ProductRunnerError>>()?;
                evidence.sort_by_key(RequirementEvidence::requirement_id);
                peritus_obligations::qualify(ledger, candidate, &[], &evidence)
                    .map(RunQualification::Legacy)
                    .map_err(invalid)
            }
            ObligationLedger::SourceBacked { root, .. } => Ok(RunQualification::SourceBacked(
                SourceObligationQualification::direct(
                    *root,
                    *candidate,
                    digest(b"peritus-product-obligation-evidence-v2", evidence_text.as_bytes()),
                    satisfied,
                ),
            )),
        }
    }

    pub(super) fn render(&self) -> String {
        match &self.ledger {
            ObligationLedger::Legacy(ledger) => {
                let mut output = String::from(
                    "Literal public obligation ledger (each clause remains acceptance-critical):\n",
                );
                for (index, entry) in ledger.entries().iter().enumerate() {
                    let clause = String::from_utf8_lossy(entry.clause().exact());
                    let _ = write!(output, "  {}. {}", index + 1, clause.trim());
                    output.push('\n');
                }
                bundle::limit_text(&output, 256 * 1024)
            }
            ObligationLedger::SourceBacked { projection, .. } => projection.clone(),
        }
    }

    pub(super) fn append_report(report: &mut String, qualification: &RunQualification) {
        match qualification {
            RunQualification::Legacy(qualification) => {
                let _ = write!(
                    report,
                    "\nPublic obligations: {}\n  required: {}\n  satisfied: {}\n  missing: {}\n  stale: {}\n  invalid: {}\n",
                    if qualification.qualified() { "PASS" } else { "FAIL" },
                    qualification.required_count(),
                    qualification.satisfied_count(),
                    qualification.missing_count(),
                    qualification.stale_count(),
                    qualification.invalid_count(),
                );
            }
            RunQualification::SourceBacked(qualification) => {
                let _ = write!(
                    report,
                    "\nPublic obligations: {}\n  authoritative source root: {}\n  coverage: exact root, candidate, and evidence binding\n",
                    if qualification.qualified() { "PASS" } else { "FAIL" },
                    digest_hex(qualification.ledger_digest()),
                );
            }
        }
    }

    pub(super) const fn source_root_digest(&self) -> Option<Sha256Digest> {
        match &self.ledger {
            ObligationLedger::Legacy(_) => None,
            ObligationLedger::SourceBacked { root, .. } => Some(root.digest()),
        }
    }

    pub(super) fn source_contract_is_current(&self, input: &ProductRunInput) -> bool {
        match &self.ledger {
            ObligationLedger::Legacy(_) => true,
            ObligationLedger::SourceBacked { root, .. } => {
                input.conversation.revision() == root.conversation_revision()
                    && input.conversation.request_source_revision().is_ok_and(|revision| {
                        revision == root.conversation_revision()
                    })
                    && input.conversation.request_source_binding()
                        == *root.source_binding().as_bytes()
                    && input.conversation.request_source_catalog_binding()
                        == *root.catalog_binding().as_bytes()
            }
        }
    }

    pub(super) const fn source_revision(&self) -> Option<u64> {
        match &self.ledger {
            ObligationLedger::Legacy(_) => None,
            ObligationLedger::SourceBacked { root, .. } => Some(root.conversation_revision()),
        }
    }

    pub(super) fn source_root_for_retained_revision(
        &self,
        input: &ProductRunInput,
        revision: u64,
    ) -> Option<Sha256Digest> {
        if self.source_revision() == Some(revision) && self.source_contract_is_current(input) {
            self.source_root_digest()
        } else {
            None
        }
    }
}

struct SourceCapture {
    revision: u64,
    page_count: u64,
    previous_page: Sha256Digest,
    projection: String,
    projected_body_bytes: u64,
    authoritative_body_bytes: u64,
}

impl SourceCapture {
    fn new(revision: u64) -> Self {
        Self {
            revision,
            page_count: 0,
            previous_page: Sha256Digest::new([0; 32]),
            projection: String::from(
                "Source-backed public obligations (this is a bounded display projection; the complete immutable request sources remain authoritative):\n",
            ),
            projected_body_bytes: 0,
            authoritative_body_bytes: 0,
        }
    }

    fn append_page(
        &mut self,
        input: &ProductRunInput,
        store: &mut ObligationStore,
        source: u64,
        source_digest: Sha256Digest,
        byte_start: u64,
        exact: &[u8],
    ) -> Result<(), ProductRunnerError> {
        self.page_count = self
            .page_count
            .checked_add(1)
            .ok_or_else(|| invariant("the source-backed obligation page frontier overflowed"))?;
        let page = SourceObligationPage::new(
            self.page_count,
            source,
            source_digest,
            self.revision,
            byte_start,
            exact,
            self.previous_page,
        )
        .map_err(invalid)?;
        store.append_page(input, page)?;
        self.previous_page = page.digest();
        Ok(())
    }

    fn append_projection(&mut self, text: &str) {
        let remaining = PROJECTION_BODY_BYTES.saturating_sub(self.projection.len());
        let end = text.floor_char_boundary(remaining.min(text.len()));
        self.projection.push_str(&text[..end]);
        self.projected_body_bytes = self
            .projected_body_bytes
            .saturating_add(u64::try_from(end).unwrap_or(u64::MAX));
    }

    fn append_projection_metadata(&mut self, text: &str) {
        let remaining = PROJECTION_BODY_BYTES.saturating_sub(self.projection.len());
        let end = text.floor_char_boundary(remaining.min(text.len()));
        self.projection.push_str(&text[..end]);
    }

    fn finish_projection(&mut self, root: SourceObligationLedgerRoot, newly_published: bool) {
        let omitted = self.authoritative_body_bytes.saturating_sub(self.projected_body_bytes);
        let _ = write!(
            self.projection,
            "\nAuthoritative obligation root: {} ({} source pages, {} bytes{}; {} display bytes omitted).\n",
            digest_hex(root.digest()),
            root.page_count(),
            self.authoritative_body_bytes,
            if newly_published { ", newly published" } else { "" },
            omitted,
        );
    }
}

fn capture_catalog(
    input: &ProductRunInput,
    store: &mut ObligationStore,
    capture: &mut SourceCapture,
    mut page: crate::ContextSourcePage,
) -> Result<(), ProductRunnerError> {
    let mut after = None;
    loop {
        for source in page.sources() {
            if after.is_some_and(|after| source.ordinal() <= after) {
                return Err(host(
                    "read authoritative request-source catalog",
                    "source ordinals did not advance monotonically",
                ));
            }
            after = Some(source.ordinal());
            if source.kind() == ContextSourceKind::UserRequest {
                capture_source(input, store, capture, source)?;
            }
        }
        let Some(next) = page.next() else { break };
        if after != Some(next) {
            return Err(host(
                "read authoritative request-source catalog",
                "the request-source continuation cursor did not match its last source",
            ));
        }
        check_cancelled(input)?;
        page = input
            .conversation
            .request_sources(Some(next))
            .map_err(|detail| host("continue authoritative request-source catalog", detail))?;
    }
    Ok(())
}

fn capture_source(
    input: &ProductRunInput,
    store: &mut ObligationStore,
    capture: &mut SourceCapture,
    source: &ContextSource,
) -> Result<(), ProductRunnerError> {
    capture.authoritative_body_bytes = capture
        .authoritative_body_bytes
        .checked_add(source.bytes())
        .ok_or_else(|| invariant("the authoritative request-source byte count overflowed"))?;
    if capture.projection.len() < PROJECTION_BODY_BYTES {
        capture.append_projection_metadata("\n[User request ");
        capture.append_projection_metadata(&source.ordinal().to_string());
        capture.append_projection_metadata(": ");
        capture.append_projection_metadata(source.label());
        capture.append_projection_metadata("]\n");
    }
    let source_digest = Sha256Digest::new(*source.digest());
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    let mut pending = Vec::with_capacity(SOURCE_PAGE_BYTES * 2);
    let mut page_start = 0_u64;
    if source.bytes() == 0 {
        let slice = input
            .conversation
            .read_request_source(source.ordinal(), 0)
            .map_err(|detail| host("read authoritative request source", detail))?;
        validate_slice(source, &slice, 0)?;
    }
    while offset < source.bytes() {
        check_cancelled(input)?;
        let slice = input
            .conversation
            .read_request_source(source.ordinal(), offset)
            .map_err(|detail| host("read authoritative request source", detail))?;
        let end = validate_slice(source, &slice, offset)?;
        hasher.update(slice.text().as_bytes());
        capture.append_projection(slice.text());
        pending.extend_from_slice(slice.text().as_bytes());
        while pending.len() >= SOURCE_PAGE_BYTES {
            capture.append_page(
                input,
                store,
                source.ordinal(),
                source_digest,
                page_start,
                &pending[..SOURCE_PAGE_BYTES],
            )?;
            pending = pending.split_off(SOURCE_PAGE_BYTES);
            page_start = page_start
                .checked_add(u64::try_from(SOURCE_PAGE_BYTES).map_err(invalid)?)
                .ok_or_else(|| invariant("the request-source page offset overflowed"))?;
        }
        offset = end;
    }
    if !pending.is_empty() {
        capture.append_page(
            input,
            store,
            source.ordinal(),
            source_digest,
            page_start,
            &pending,
        )?;
    }
    let observed = Sha256Digest::new(hasher.finalize().into());
    if offset != source.bytes() || observed != source_digest {
        return Err(host(
            "verify authoritative request source",
            "the exact body did not match its immutable length and digest",
        ));
    }
    Ok(())
}

fn capture_inline(
    input: &ProductRunInput,
    store: &mut ObligationStore,
    capture: &mut SourceCapture,
) -> Result<(), ProductRunnerError> {
    let transcript = input.conversation.render();
    let exact = transcript.as_bytes();
    capture.authoritative_body_bytes = u64::try_from(exact.len()).map_err(invalid)?;
    capture.append_projection(&transcript);
    let source_digest = Sha256Digest::new(Sha256::digest(exact).into());
    for (index, clause) in exact.chunks(SOURCE_PAGE_BYTES).enumerate() {
        let start = index
            .checked_mul(SOURCE_PAGE_BYTES)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| invariant("the rendered conversation page offset overflowed"))?;
        capture.append_page(input, store, 1, source_digest, start, clause)?;
    }
    Ok(())
}

fn validate_slice(
    source: &ContextSource,
    slice: &crate::ContextSourceSlice,
    expected_offset: u64,
) -> Result<u64, ProductRunnerError> {
    let length = u64::try_from(slice.text().len()).map_err(invalid)?;
    let end = expected_offset
        .checked_add(length)
        .ok_or_else(|| invariant("the request-source slice offset overflowed"))?;
    let expected_next = (end < source.bytes()).then_some(end);
    if slice.source() != source.ordinal()
        || slice.offset() != expected_offset
        || end > source.bytes()
        || slice.next() != expected_next
        || (source.bytes() > 0 && slice.text().is_empty())
    {
        return Err(host(
            "read authoritative request source",
            "the host returned a slice outside its immutable descriptor",
        ));
    }
    Ok(end)
}

fn clause_spans(text: &str, maximum: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let proposed = start.saturating_add(maximum).min(text.len());
        let mut end = text.floor_char_boundary(proposed);
        if end == start {
            end = text.ceil_char_boundary(start.saturating_add(1).min(text.len()));
        }
        spans.push((start, end));
        start = end;
    }
    spans
}

/// Identify an occurrence without conflating repeated literal clauses.
///
/// The fixed-width source, revision, and span fields retain exact provenance.
/// This domain applies only to newly derived ledgers; retained ledger and
/// evidence encodings keep their original identities and canonical bytes.
fn occurrence_id(
    source: &PublicTaskSource,
    start: usize,
    end: usize,
    clause: &[u8],
) -> Result<RequirementId, ProductRunnerError> {
    let start = u64::try_from(start).map_err(invalid)?;
    let end = u64::try_from(end).map_err(invalid)?;
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-obligation-occurrence-v2\0");
    hasher.update(source.digest().as_bytes());
    hasher.update(source.conversation_revision().to_le_bytes());
    hasher.update(start.to_le_bytes());
    hasher.update(end.to_le_bytes());
    hasher.update(Sha256::digest(clause));
    Ok(RequirementId::new(Sha256Digest::new(hasher.finalize().into())))
}

fn digest(domain: &[u8], bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(u64::try_from(domain.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(domain);
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
    Sha256Digest::new(hasher.finalize().into())
}

fn invalid(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "construct public obligation ledger",
        error.to_string(),
    )
}

fn host(operation: &'static str, detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::InvalidPrecondition, operation, detail)
}

fn invariant(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "construct source-backed public obligation ledger",
        detail,
    )
}

fn digest_hex(digest: Sha256Digest) -> String {
    digest.as_bytes().iter().fold(String::with_capacity(64), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_types::{RunId, WorkspaceId};

    #[test]
    fn exact_public_text_becomes_a_qualified_requirement_only_after_review() {
        let obligations =
            RunObligations::capture("User:\nImplement exact behavior.", 3).expect("obligations");
        let candidate = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            Sha256Digest::new([3; 32]),
            None,
            3,
            1,
        )
        .expect("candidate");

        let pending = obligations.qualify(&candidate, true, false, "gates pass").expect("pending");
        let accepted = obligations
            .qualify(&candidate, true, true, "gates pass; review pass")
            .expect("accepted");

        assert!(!pending.qualified());
        assert!(accepted.qualified());
        assert!(obligations.render().contains("Implement exact behavior."));
    }

    #[test]
    fn failed_obligation_routes_only_candidate_defects_to_the_fixer() {
        let obligations =
            RunObligations::capture("User:\nImplement exact behavior.", 1).expect("obligations");
        let candidate = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            Sha256Digest::new([3; 32]),
            None,
            1,
            1,
        )
        .expect("candidate");
        let report = obligations.qualify(&candidate, false, false, "failed").expect("report");

        assert_eq!(
            QualificationState::new(false, report.qualified(), false).fixer_disposition(true),
            FailureDisposition::RequestFixer,
        );
    }
}
