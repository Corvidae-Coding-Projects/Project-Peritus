//! Executable E2 fact projections and Verus proof roots.

#![allow(
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    reason = "formal fact projections keep each independent invariant premise explicit"
)]

#[doc = " Executable independent facts for manifest selection containment."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionContainmentFacts {
    #[doc = " Every entry belongs to exactly one frozen subject."]
    exact_subject: bool,
    #[doc = " Every entry matches its immutable C7 observation."]
    exact_trace_event: bool,
    #[doc = " Every entry matches its checked C0 record."]
    exact_journal_record: bool,
    #[doc = " Every added ancestor belongs to the same subject."]
    closure_within_subject: bool,
    #[doc = " Every selected count is inside the frozen limit."]
    bounded: bool,
}
impl SelectionContainmentFacts {
    #[doc = " Creates a complete selection fact projection."]
    #[must_use]
    pub const fn new(
        exact_subject: bool,
        exact_trace_event: bool,
        exact_journal_record: bool,
        closure_within_subject: bool,
        bounded: bool,
    ) -> Self {
        Self {
            exact_subject,
            exact_trace_event,
            exact_journal_record,
            closure_within_subject,
            bounded,
        }
    }
    #[doc = " Returns whether subject ownership is exact."]
    #[must_use]
    pub const fn exact_subject(self) -> bool {
        self.exact_subject
    }
    #[doc = " Returns whether trace identity is exact."]
    #[must_use]
    pub const fn exact_trace_event(self) -> bool {
        self.exact_trace_event
    }
    #[doc = " Returns whether C0 provenance is exact."]
    #[must_use]
    pub const fn exact_journal_record(self) -> bool {
        self.exact_journal_record
    }
    #[doc = " Returns whether closure remains within one subject."]
    #[must_use]
    pub const fn closure_within_subject(self) -> bool {
        self.closure_within_subject
    }
    #[doc = " Returns whether selection counts are bounded."]
    #[must_use]
    pub const fn bounded(self) -> bool {
        self.bounded
    }
}
#[doc = " Proves the executable conjunction is exactly selection containment."]
#[must_use]
pub const fn selection_containment(facts: SelectionContainmentFacts) -> bool {
    facts.exact_subject
        && facts.exact_trace_event
        && facts.exact_journal_record
        && facts.closure_within_subject
        && facts.bounded
}
#[doc = " Executable facts for one validated citation."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CitationContainmentFacts {
    #[doc = " Citation and manifest identities match."]
    manifest_matches: bool,
    #[doc = " The event exists exactly once in the manifest."]
    selected_event: bool,
    #[doc = " Complete subject/revision/environment bindings match."]
    subject_matches: bool,
    #[doc = " Event position and frame digest match."]
    frame_matches: bool,
    #[doc = " Optional artifact is ordinary, selected, finalized, and source-related."]
    artifact_selected: bool,
    #[doc = " Optional half-open artifact range is nonempty and in bounds."]
    artifact_range_valid: bool,
}
impl CitationContainmentFacts {
    #[doc = " Creates a complete citation fact projection."]
    #[must_use]
    pub const fn new(
        manifest_matches: bool,
        selected_event: bool,
        subject_matches: bool,
        frame_matches: bool,
        artifact_selected: bool,
        artifact_range_valid: bool,
    ) -> Self {
        Self {
            manifest_matches,
            selected_event,
            subject_matches,
            frame_matches,
            artifact_selected,
            artifact_range_valid,
        }
    }
    #[doc = " Returns whether manifest identity matches."]
    #[must_use]
    pub const fn manifest_matches(self) -> bool {
        self.manifest_matches
    }
    #[doc = " Returns whether the event is selected."]
    #[must_use]
    pub const fn selected_event(self) -> bool {
        self.selected_event
    }
    #[doc = " Returns whether subject bindings match."]
    #[must_use]
    pub const fn subject_matches(self) -> bool {
        self.subject_matches
    }
    #[doc = " Returns whether frame provenance matches."]
    #[must_use]
    pub const fn frame_matches(self) -> bool {
        self.frame_matches
    }
    #[doc = " Returns whether optional artifact identity is selected."]
    #[must_use]
    pub const fn artifact_selected(self) -> bool {
        self.artifact_selected
    }
    #[doc = " Returns whether optional artifact range is contained."]
    #[must_use]
    pub const fn artifact_range_valid(self) -> bool {
        self.artifact_range_valid
    }
}
#[doc = " Proves validated citations remain inside selected redacted evidence."]
#[must_use]
pub const fn citation_containment(facts: CitationContainmentFacts) -> bool {
    facts.manifest_matches
        && facts.selected_event
        && facts.subject_matches
        && facts.frame_matches
        && facts.artifact_selected
        && facts.artifact_range_valid
}
#[doc = " Executable facts for complete report validity."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportValidityFacts {
    #[doc = " Report binds the exact immutable manifest and query."]
    binding_exact: bool,
    #[doc = " Every collection is canonical and duplicate-free."]
    canonical: bool,
    #[doc = " Every collection and encoded byte string is bounded."]
    bounded: bool,
    #[doc = " Every taxonomy value is in the closed schema-v1 catalog."]
    taxonomy_valid: bool,
    #[doc = " Every supported statement meets its citation and uncertainty obligations."]
    claims_supported: bool,
    #[doc = " Health and recommendations remain diagnostic and non-authoritative."]
    diagnostic_only: bool,
}
impl ReportValidityFacts {
    #[doc = " Creates a complete report-validity fact projection."]
    #[must_use]
    pub const fn new(
        binding_exact: bool,
        canonical: bool,
        bounded: bool,
        taxonomy_valid: bool,
        claims_supported: bool,
        diagnostic_only: bool,
    ) -> Self {
        Self {
            binding_exact,
            canonical,
            bounded,
            taxonomy_valid,
            claims_supported,
            diagnostic_only,
        }
    }
    #[doc = " Returns whether report bindings are exact."]
    #[must_use]
    pub const fn binding_exact(self) -> bool {
        self.binding_exact
    }
    #[doc = " Returns whether report order and identities are canonical."]
    #[must_use]
    pub const fn canonical(self) -> bool {
        self.canonical
    }
    #[doc = " Returns whether report resources are bounded."]
    #[must_use]
    pub const fn bounded(self) -> bool {
        self.bounded
    }
    #[doc = " Returns whether taxonomy tags are valid."]
    #[must_use]
    pub const fn taxonomy_valid(self) -> bool {
        self.taxonomy_valid
    }
    #[doc = " Returns whether claim evidence obligations hold."]
    #[must_use]
    pub const fn claims_supported(self) -> bool {
        self.claims_supported
    }
    #[doc = " Returns whether output remains diagnostic only."]
    #[must_use]
    pub const fn diagnostic_only(self) -> bool {
        self.diagnostic_only
    }
}
#[doc = " Proves report validity from the same executable fact projection used by refinement tests."]
#[must_use]
pub const fn report_validity(facts: ReportValidityFacts) -> bool {
    facts.binding_exact
        && facts.canonical
        && facts.bounded
        && facts.taxonomy_valid
        && facts.claims_supported
        && facts.diagnostic_only
}
#[doc = " Executable replay and terminal-dominance facts."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayEquivalenceFacts {
    #[doc = " Replay consumed the complete canonical prefix."]
    complete_prefix: bool,
    #[doc = " Pure replay state equals the checked checkpoint."]
    state_equal: bool,
    #[doc = " Replay and checkpoint digests match."]
    digest_equal: bool,
    #[doc = " A terminal state did not advance to later success."]
    terminal_dominates: bool,
}
impl ReplayEquivalenceFacts {
    #[doc = " Creates a complete replay-equivalence fact projection."]
    #[must_use]
    pub const fn new(
        complete_prefix: bool,
        state_equal: bool,
        digest_equal: bool,
        terminal_dominates: bool,
    ) -> Self {
        Self { complete_prefix, state_equal, digest_equal, terminal_dominates }
    }
    #[doc = " Returns whether replay consumed the complete prefix."]
    #[must_use]
    pub const fn complete_prefix(self) -> bool {
        self.complete_prefix
    }
    #[doc = " Returns whether semantic states match."]
    #[must_use]
    pub const fn state_equal(self) -> bool {
        self.state_equal
    }
    #[doc = " Returns whether state digests match."]
    #[must_use]
    pub const fn digest_equal(self) -> bool {
        self.digest_equal
    }
    #[doc = " Returns whether terminal state dominates late success."]
    #[must_use]
    pub const fn terminal_dominates(self) -> bool {
        self.terminal_dominates
    }
}
#[doc = " Proves exact replay equivalence and terminal dominance from executable facts."]
#[must_use]
pub const fn replay_equivalence(facts: ReplayEquivalenceFacts) -> bool {
    facts.complete_prefix && facts.state_equal && facts.digest_equal && facts.terminal_dominates
}
#[doc = " Executable facts covering every independently bounded E2 collection."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedAnalysisFacts {
    #[doc = " Query and selection collections are bounded."]
    selection: bool,
    #[doc = " Timeline entries and diagnostics are bounded."]
    timelines: bool,
    #[doc = " Causes, alternatives, contrary evidence, and claims are bounded."]
    causes_and_claims: bool,
    #[doc = " Patterns, members, and component links are bounded."]
    patterns_and_components: bool,
    #[doc = " Model stream, input, output, attempts, retries, and tokens are bounded."]
    model: bool,
    #[doc = " Canonical report, durable event, and complete state bytes are bounded."]
    encoded_state: bool,
}
impl BoundedAnalysisFacts {
    #[doc = " Creates a complete bounded-analysis fact projection."]
    #[must_use]
    pub const fn new(
        selection: bool,
        timelines: bool,
        causes_and_claims: bool,
        patterns_and_components: bool,
        model: bool,
        encoded_state: bool,
    ) -> Self {
        Self {
            selection,
            timelines,
            causes_and_claims,
            patterns_and_components,
            model,
            encoded_state,
        }
    }
    #[doc = " Returns whether selection is bounded."]
    #[must_use]
    pub const fn selection(self) -> bool {
        self.selection
    }
    #[doc = " Returns whether timelines are bounded."]
    #[must_use]
    pub const fn timelines(self) -> bool {
        self.timelines
    }
    #[doc = " Returns whether causes and claims are bounded."]
    #[must_use]
    pub const fn causes_and_claims(self) -> bool {
        self.causes_and_claims
    }
    #[doc = " Returns whether patterns and component links are bounded."]
    #[must_use]
    pub const fn patterns_and_components(self) -> bool {
        self.patterns_and_components
    }
    #[doc = " Returns whether optional model analysis is bounded."]
    #[must_use]
    pub const fn model(self) -> bool {
        self.model
    }
    #[doc = " Returns whether encoded state is bounded."]
    #[must_use]
    pub const fn encoded_state(self) -> bool {
        self.encoded_state
    }
}
#[doc = " Proves that every represented E2 resource class satisfies its frozen ceiling."]
#[must_use]
pub const fn bounded_analysis(facts: BoundedAnalysisFacts) -> bool {
    facts.selection
        && facts.timelines
        && facts.causes_and_claims
        && facts.patterns_and_components
        && facts.model
        && facts.encoded_state
}
#[doc = " Executable absence-of-authority and evidence-nonmutation facts."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonAuthorityFacts {
    #[doc = " No E1 mutation or replacement bytes are representable."]
    no_harness_mutation: bool,
    #[doc = " No acceptance, waiver, or approval decision is representable."]
    no_acceptance_authority: bool,
    #[doc = " No evaluation or statistical verdict is representable."]
    no_evaluation_authority: bool,
    #[doc = " No promotion, activation, rollback, or production pointer is representable."]
    no_promotion_authority: bool,
    #[doc = " Report construction leaves source evidence unchanged."]
    source_evidence_unchanged: bool,
    #[doc = " Default report values carry no raw-vault bytes."]
    no_raw_vault_bytes: bool,
}
impl NonAuthorityFacts {
    #[doc = " Creates a complete non-authority fact projection."]
    #[must_use]
    pub const fn new(
        no_harness_mutation: bool,
        no_acceptance_authority: bool,
        no_evaluation_authority: bool,
        no_promotion_authority: bool,
        source_evidence_unchanged: bool,
        no_raw_vault_bytes: bool,
    ) -> Self {
        Self {
            no_harness_mutation,
            no_acceptance_authority,
            no_evaluation_authority,
            no_promotion_authority,
            source_evidence_unchanged,
            no_raw_vault_bytes,
        }
    }
    #[doc = " Returns whether E1 mutation is absent."]
    #[must_use]
    pub const fn no_harness_mutation(self) -> bool {
        self.no_harness_mutation
    }
    #[doc = " Returns whether acceptance authority is absent."]
    #[must_use]
    pub const fn no_acceptance_authority(self) -> bool {
        self.no_acceptance_authority
    }
    #[doc = " Returns whether evaluation authority is absent."]
    #[must_use]
    pub const fn no_evaluation_authority(self) -> bool {
        self.no_evaluation_authority
    }
    #[doc = " Returns whether promotion authority is absent."]
    #[must_use]
    pub const fn no_promotion_authority(self) -> bool {
        self.no_promotion_authority
    }
    #[doc = " Returns whether source evidence is unchanged."]
    #[must_use]
    pub const fn source_evidence_unchanged(self) -> bool {
        self.source_evidence_unchanged
    }
    #[doc = " Returns whether raw vault bytes are absent."]
    #[must_use]
    pub const fn no_raw_vault_bytes(self) -> bool {
        self.no_raw_vault_bytes
    }
}
#[doc = " Proves checked outputs carry diagnostic evidence without authority or source mutation."]
#[must_use]
pub const fn non_authority(facts: NonAuthorityFacts) -> bool {
    facts.no_harness_mutation
        && facts.no_acceptance_authority
        && facts.no_evaluation_authority
        && facts.no_promotion_authority
        && facts.source_evidence_unchanged
        && facts.no_raw_vault_bytes
}
