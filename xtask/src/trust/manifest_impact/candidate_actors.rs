//! Candidate-bound actor registry validation for the authorization phase.
//!
//! The protected checkout remains unchanged while the proposed registry is read from the
//! detached verdict's exact implementation tree. Only a semantic append of the PCR's fresh
//! owner and reviewer is accepted; protected identities and provenance cannot be rewritten.

use super::{MANIFEST, sha256_hex};
use crate::error::Diagnostic;
use crate::trust::manifest_actor::{self, ActorRegistry};
use crate::trust::manifest_actor_model::{
    ActorEntry, ActorProvenanceDocument, ActorProvenanceEntry, ActorRole, ActorsDocument,
};
use crate::trust::manifest_file;
use crate::trust::manifest_model::{ProofImpactChange, ProofImpactVerdict, ProofSourceChange};
use std::path::Path;

#[path = "candidate_actors/git.rs"]
mod git;

const ACTORS_PATH: &str = "verification/actors.toml";
const PROVENANCE_PATH: &str = "verification/actor-provenance.json";

/// Actor documents owned independently of the protected checkout and borrowed only after load.
pub(super) struct CandidateActors {
    actors: ActorsDocument,
    provenance: ActorProvenanceDocument,
    provenance_sha256: String,
}

impl CandidateActors {
    /// Loads and bounds the proposed registry to the reviewed implementation tree and PCR.
    pub(super) fn load(
        root: &Path,
        protected_actors: &ActorsDocument,
        change: &ProofImpactChange,
        verdict: &ProofImpactVerdict,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Self> {
        let protected_actor_bytes =
            manifest_file::read_bytes(root, Path::new(ACTORS_PATH), diagnostics);
        let protected_provenance =
            manifest_file::read_json(root, Path::new(PROVENANCE_PATH), diagnostics);
        if !git::tree_object_exists(root, &verdict.implementation_tree, MANIFEST, diagnostics) {
            return None;
        }
        let candidate_actors = git::load_tree_toml::<ActorsDocument>(
            root,
            &verdict.implementation_tree,
            ACTORS_PATH,
            diagnostics,
        );
        let candidate_provenance = git::load_tree_json::<ActorProvenanceDocument>(
            root,
            &verdict.implementation_tree,
            PROVENANCE_PATH,
            diagnostics,
        );
        let (Some(protected_actor_bytes), Some((protected_provenance, protected_provenance_bytes))) =
            (protected_actor_bytes, protected_provenance)
        else {
            return None;
        };
        let (Some((actors, actor_bytes)), Some((provenance, provenance_bytes))) =
            (candidate_actors, candidate_provenance)
        else {
            return None;
        };
        let provenance_sha256 = sha256_hex(&provenance_bytes);
        validate_append_only(
            protected_actors,
            &protected_provenance,
            &actors,
            &provenance,
            &provenance_sha256,
            change,
            diagnostics,
        );
        validate_pcr_binding(
            change,
            ACTORS_PATH,
            &protected_actor_bytes,
            &actor_bytes,
            diagnostics,
        );
        validate_pcr_binding(
            change,
            PROVENANCE_PATH,
            &protected_provenance_bytes,
            &provenance_bytes,
            diagnostics,
        );
        validate_reviewer_principal(&actors, change, verdict, diagnostics);
        Some(Self { actors, provenance, provenance_sha256 })
    }

    /// Reuses the ordinary registry validator over the detached, owned candidate documents.
    pub(super) fn registry(&self, diagnostics: &mut Vec<Diagnostic>) -> ActorRegistry<'_> {
        manifest_actor::validate_documents(
            &self.actors,
            &self.provenance,
            &self.provenance_sha256,
            diagnostics,
        )
    }
}

fn validate_append_only(
    protected_actors: &ActorsDocument,
    protected_provenance: &ActorProvenanceDocument,
    candidate_actors: &ActorsDocument,
    candidate_provenance: &ActorProvenanceDocument,
    candidate_provenance_sha256: &str,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let owner_is_fresh = !protected_actors.entries.iter().any(|actor| actor.id == change.owner);
    let reviewer_is_fresh =
        !protected_actors.entries.iter().any(|actor| actor.id == change.reviewer);
    let expected_new = usize::from(owner_is_fresh) + 1;
    let actor_shape =
        candidate_actors.entries.len() == protected_actors.entries.len() + expected_new;
    let provenance_shape =
        candidate_provenance.entries.len() == protected_provenance.entries.len() + expected_new;
    if !actor_shape || !provenance_shape {
        diagnostics.push(Diagnostic::at(
            ACTORS_PATH,
            format!(
                "candidate for `{}` does not append exactly its required fresh identities",
                change.id
            ),
            "retain history, append one fresh reviewer, and append the owner only when its durable identity is not registered",
        ));
    }
    if !reviewer_is_fresh {
        diagnostics.push(Diagnostic::at(
            ACTORS_PATH,
            format!("candidate reviewer `{}` is not fresh for `{}`", change.reviewer, change.id),
            "append an independent reviewer identity created for this candidate review; protected reviewers cannot be reused",
        ));
    }

    for (protected, candidate) in protected_actors.entries.iter().zip(&candidate_actors.entries) {
        if !same_actor_except_provenance_digest(protected, candidate)
            || candidate.provenance.record_sha256 != candidate_provenance_sha256
        {
            diagnostics.push(Diagnostic::at(
                ACTORS_PATH,
                format!("candidate rewrites protected actor `{}`", protected.id),
                "preserve every historical actor field; only refresh its whole-provenance digest reference",
            ));
        }
    }
    for (protected, candidate) in
        protected_provenance.entries.iter().zip(&candidate_provenance.entries)
    {
        if !same_provenance(protected, candidate) {
            diagnostics.push(Diagnostic::at(
                PROVENANCE_PATH,
                format!("candidate rewrites protected provenance for `{}`", protected.actor_id),
                "preserve every historical provenance field byte-semantically and append new records",
            ));
        }
    }

    let new_actors =
        candidate_actors.entries.get(protected_actors.entries.len()..).unwrap_or_default();
    let new_provenance =
        candidate_provenance.entries.get(protected_provenance.entries.len()..).unwrap_or_default();
    if owner_is_fresh {
        validate_fresh_actor(
            protected_actors,
            new_actors,
            new_provenance,
            &change.owner,
            ActorRole::Owner,
            diagnostics,
        );
    }
    validate_fresh_actor(
        protected_actors,
        new_actors,
        new_provenance,
        &change.reviewer,
        ActorRole::Reviewer,
        diagnostics,
    );
}

fn validate_fresh_actor(
    protected: &ActorsDocument,
    new_actors: &[ActorEntry],
    new_provenance: &[ActorProvenanceEntry],
    actor_id: &str,
    role: ActorRole,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actors: Vec<_> = new_actors.iter().filter(|actor| actor.id == actor_id).collect();
    let provenance_count = new_provenance.iter().filter(|entry| entry.actor_id == actor_id).count();
    let exact_role = actors.first().is_some_and(|actor| actor.roles.as_slice() == [role]);
    if actors.len() != 1 || provenance_count != 1 || !exact_role {
        diagnostics.push(Diagnostic::at(
            ACTORS_PATH,
            format!(
                "PCR actor `{actor_id}` is not one fresh, provenance-backed `{}` identity",
                role.as_str()
            ),
            "append the PCR owner and reviewer once, each with exactly its distinct required role",
        ));
        return;
    }
    let candidate = actors[0];
    if protected.entries.iter().any(|historical| {
        historical.id == candidate.id
            || historical.kind == candidate.kind && historical.principal == candidate.principal
    }) {
        diagnostics.push(Diagnostic::at(
            ACTORS_PATH,
            format!("PCR actor `{actor_id}` impersonates a protected identity"),
            "register a genuinely fresh provider principal without changing or reusing historical actors",
        ));
    }
}

fn validate_reviewer_principal(
    actors: &ActorsDocument,
    change: &ProofImpactChange,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let matching: Vec<_> =
        actors.entries.iter().filter(|actor| actor.id == change.reviewer).collect();
    if matching.len() != 1 || matching[0].principal != verdict.reviewer_principal {
        diagnostics.push(Diagnostic::at(
            change.verdict.as_ref().map_or_else(
                || MANIFEST.to_owned(),
                |reference| reference.path.clone(),
            ),
            format!(
                "candidate reviewer identity for `{}` does not match the detached verdict principal",
                change.id
            ),
            "bind the verdict to the exact fresh reviewer principal in the candidate actor registry",
        ));
    }
}

fn validate_pcr_binding(
    change: &ProofImpactChange,
    path: &str,
    protected_bytes: &[u8],
    candidate_bytes: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let matching: Vec<_> =
        change.source_changes.iter().filter(|source| source.source_file == path).collect();
    let exact = matching.len() == 1
        && matching.first().is_some_and(|transition| {
            transition_matches(transition, protected_bytes, candidate_bytes)
        });
    if !exact {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` does not exactly bind candidate `{path}` bytes", change.id),
            "record exactly one previous/current transition from protected raw bytes to the reviewed tree blob",
        ));
    }
}

fn transition_matches(
    transition: &ProofSourceChange,
    protected_bytes: &[u8],
    candidate_bytes: &[u8],
) -> bool {
    transition.previous.as_ref().map(|snapshot| snapshot.sha256.as_str())
        == Some(sha256_hex(protected_bytes).as_str())
        && transition.current.as_ref().map(|snapshot| snapshot.sha256.as_str())
            == Some(sha256_hex(candidate_bytes).as_str())
}

fn same_actor_except_provenance_digest(left: &ActorEntry, right: &ActorEntry) -> bool {
    left.id == right.id
        && left.kind == right.kind
        && left.principal == right.principal
        && left.display_name == right.display_name
        && left.roles == right.roles
        && left.provenance.record_path == right.provenance.record_path
}

fn same_provenance(left: &ActorProvenanceEntry, right: &ActorProvenanceEntry) -> bool {
    left.actor_id == right.actor_id
        && left.kind == right.kind
        && left.principal == right.principal
        && left.repository == right.repository
        && left.issue == right.issue
        && left.issue_created_at == right.issue_created_at
        && left.session == right.session
        && left.task == right.task
        && left.mode == right.mode
        && left.model == right.model
        && left.reasoning_effort == right.reasoning_effort
        && left.public_key == right.public_key
        && left.allowed_signer == right.allowed_signer
        && left.record_locators == right.record_locators
}

#[cfg(test)]
#[path = "candidate_actors/tests.rs"]
mod tests;
