use super::manifest_actor_model::{
    ActorEntry, ActorKind, ActorMode, ActorProvenanceDocument, ActorProvenanceEntry, ActorRole,
    ActorsDocument,
};
use super::manifest_file;
use super::manifest_support::{validate_envelope, validate_id, validate_text};
use crate::error::Diagnostic;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MANIFEST: &str = "verification/actors.toml";
const PROVENANCE_RECORD: &str = "verification/actor-provenance.json";
const REPOSITORY: &str = "Corvidae-Coding-Projects/Project-Peritus";

pub(super) struct ActorRegistry<'a> {
    entries: BTreeMap<&'a str, &'a ActorEntry>,
}

impl ActorRegistry<'_> {
    pub(super) fn validate_reference(
        &self,
        manifest: &Path,
        entry_id: &str,
        field: &str,
        actor_id: &str,
        required_role: ActorRole,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(actor) = self.entries.get(actor_id).copied() else {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{entry_id}` field `{field}` names unregistered actor `{actor_id}`"),
                "register the durable actor identity in verification/actors.toml",
            ));
            return;
        };
        if !actor.roles.contains(&required_role) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!(
                    "entry `{entry_id}` field `{field}` references actor `{actor_id}` without `{}` role",
                    required_role.as_str()
                ),
                "assign an independently reviewed role in verification/actors.toml",
            ));
        }
    }

    pub(super) fn validate_pair(
        &self,
        manifest: &Path,
        entry_id: &str,
        owner: &str,
        reviewer: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.validate_reference(manifest, entry_id, "owner", owner, ActorRole::Owner, diagnostics);
        self.validate_reference(
            manifest,
            entry_id,
            "reviewer",
            reviewer,
            ActorRole::Reviewer,
            diagnostics,
        );
        if owner == reviewer {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{entry_id}` has the same owner and reviewer actor"),
                "assign distinct registered actor identities",
            ));
            return;
        }
        let pair = self.entries.get(owner).zip(self.entries.get(reviewer));
        if pair.is_some_and(|(owner, reviewer)| actor_subject(owner) == actor_subject(reviewer)) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{entry_id}` aliases one actor as both owner and reviewer"),
                "use distinct canonical principals and provenance records",
            ));
        }
    }
}

pub(super) fn validate<'document>(
    root: &Path,
    document: &'document ActorsDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ActorRegistry<'document>> {
    let manifest = Path::new(MANIFEST);
    validate_envelope(
        manifest,
        &document.schema,
        document.schema_version,
        &document.baseline,
        "peritus.verification.actors",
        diagnostics,
    );
    let provenance = load_provenance(root, diagnostics)?;
    let mut provenance_entries = BTreeMap::new();
    validate_envelope(
        Path::new(PROVENANCE_RECORD),
        &provenance.schema,
        provenance.schema_version,
        &provenance.baseline,
        "peritus.verification.actor-provenance",
        diagnostics,
    );
    for entry in &provenance.entries {
        if provenance_entries.insert(entry.actor_id.as_str(), entry).is_some() {
            diagnostics.push(Diagnostic::at(
                PROVENANCE_RECORD,
                format!("actor provenance for `{}` is declared more than once", entry.actor_id),
                "retain one content-addressed provenance record per actor",
            ));
        }
    }
    let provenance_hash = provenance.raw_sha256.as_str();
    let mut entries = BTreeMap::new();
    let mut subjects = BTreeSet::new();
    for actor in &document.entries {
        let evidence = provenance_entries.get(actor.id.as_str()).copied();
        validate_entry(manifest, actor, evidence, provenance_hash, diagnostics);
        if entries.insert(actor.id.as_str(), actor).is_some() {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("actor ID `{}` is declared more than once", actor.id),
                "retain one canonical registry entry per durable actor identity",
            ));
        }
        if !subjects.insert(actor_subject(actor)) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("actor `{}` aliases an already registered external subject", actor.id),
                "retain one stable ACTOR-NNNN identity per provider principal and provenance locator",
            ));
        }
    }
    for actor_id in provenance_entries.keys().filter(|id| !entries.contains_key(**id)) {
        diagnostics.push(Diagnostic::at(
            PROVENANCE_RECORD,
            format!("provenance record names unregistered actor `{actor_id}`"),
            "remove the stale record or restore its actors.toml entry",
        ));
    }
    if !document.entries.iter().any(|actor| actor.roles.contains(&ActorRole::Owner))
        || !document.entries.iter().any(|actor| actor.roles.contains(&ActorRole::Reviewer))
    {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            "actor registry lacks an owner or independent reviewer",
            "register distinct durable actors for both required roles",
        ));
    }
    Some(ActorRegistry { entries })
}

fn validate_entry(
    manifest: &Path,
    actor: &ActorEntry,
    evidence: Option<&ActorProvenanceEntry>,
    provenance_hash: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_id(manifest, &actor.id, "ACTOR-", diagnostics);
    validate_text(manifest, &actor.id, "display_name", &actor.display_name, diagnostics);
    validate_text(manifest, &actor.id, "principal", &actor.principal, diagnostics);
    if !valid_principal(actor.kind, &actor.principal) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "actor `{}` has invalid `{}` principal `{}`",
                actor.id,
                actor.kind.as_str(),
                actor.principal
            ),
            "record the provider's exact durable principal",
        ));
    }
    validate_provenance_ref(manifest, actor, provenance_hash, diagnostics);
    if let Some(evidence) = evidence {
        validate_provenance(actor, evidence, diagnostics);
    } else {
        diagnostics.push(Diagnostic::at(
            PROVENANCE_RECORD,
            format!("actor `{}` has no retained provenance record", actor.id),
            "add its exact provider identity and execution record",
        ));
    }
    let roles: BTreeSet<_> = actor.roles.iter().copied().collect();
    if roles.is_empty()
        || roles.len() != actor.roles.len()
        || !actor.roles.windows(2).all(|pair| pair[0] < pair[1])
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("actor `{}` has an empty, duplicate, or noncanonical role set", actor.id),
            "list each applicable role once in ascending enum order",
        ));
    }
}

fn validate_provenance_ref(
    manifest: &Path,
    actor: &ActorEntry,
    actual_hash: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if actor.provenance.record_path != PROVENANCE_RECORD
        || !valid_sha256(&actor.provenance.record_sha256)
        || actual_hash != actor.provenance.record_sha256
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "actor `{}` has a missing or mismatched content-addressed provenance record",
                actor.id
            ),
            format!("reference `{PROVENANCE_RECORD}` and its exact lowercase raw-byte SHA-256"),
        ));
    }
}

fn validate_provenance(
    actor: &ActorEntry,
    provenance: &ActorProvenanceEntry,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let manifest = Path::new(PROVENANCE_RECORD);
    validate_text(
        manifest,
        &actor.id,
        "issue_created_at",
        &provenance.issue_created_at,
        diagnostics,
    );
    if let Some(model) = &provenance.model {
        validate_text(manifest, &actor.id, "model", model, diagnostics);
    }
    let valid_task = provenance.task == "/root"
        || provenance.task.strip_prefix("/root/").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_'))
        });
    let expected_codex_principal =
        format!("{}/session/{}/task{}", provenance.repository, provenance.session, provenance.task);
    let identity_matches = actor.kind == provenance.kind
        && actor.principal == provenance.principal
        && (actor.kind != ActorKind::CodexSubagent || actor.principal == expected_codex_principal);
    let reviewed_by_required_model = !actor.roles.contains(&ActorRole::Reviewer)
        || provenance.model.as_deref() == Some("gpt-5.6-sol")
            && provenance.reasoning_effort
                == Some(super::manifest_actor_model::ActorReasoningEffort::Xhigh);
    let role_mode = if actor.roles.contains(&ActorRole::Reviewer) {
        provenance.mode == ActorMode::ReadOnlyReview
    } else {
        provenance.mode == ActorMode::Implementation
    };
    if provenance.repository != REPOSITORY
        || provenance.issue == 0
        || provenance.session == 0
        || !valid_task
        || !valid_issue_time(&provenance.issue_created_at)
        || !identity_matches
        || !reviewed_by_required_model
        || !role_mode
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("actor `{}` has a malformed or mismatched provenance locator", actor.id),
            format!(
                "record `{REPOSITORY}`, positive issue/session IDs, exact provider identity/execution, and role-correct mode"
            ),
        ));
    }
    validate_record_locators(actor, provenance, diagnostics);
}

fn validate_record_locators(
    actor: &ActorEntry,
    provenance: &ActorProvenanceEntry,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let locators = &provenance.record_locators;
    let canonical = !locators.is_empty() && locators.windows(2).all(|pair| pair[0] < pair[1]);
    let valid = match actor.kind {
        ActorKind::CrosslinkAgent => {
            locators.as_slice() == ["embedded:allowed-signer", "embedded:public-key"]
                && crosslink_key_matches(actor, provenance)
        }
        ActorKind::CodexSubagent => {
            provenance.public_key.is_none()
                && provenance.allowed_signer.is_none()
                && locators.as_slice() == [format!("codex-collaboration:{}", actor.principal)]
        }
    };
    if !canonical || !valid {
        diagnostics.push(Diagnostic::at(
            PROVENANCE_RECORD,
            format!(
                "actor `{}` has missing, duplicate, unordered, or malformed record locators",
                actor.id
            ),
            "retain the embedded key/signer evidence or canonical Codex collaboration run locator",
        ));
    }
}

fn load_provenance(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Option<LoadedProvenance> {
    let relative = Path::new(PROVENANCE_RECORD);
    let (document, bytes) = manifest_file::read_json(root, relative, diagnostics)?;
    let raw_sha256 = super::manifest_impact::sha256_hex(&bytes);
    Some(LoadedProvenance { document, raw_sha256 })
}

struct LoadedProvenance {
    document: ActorProvenanceDocument,
    raw_sha256: String,
}

impl std::ops::Deref for LoadedProvenance {
    type Target = ActorProvenanceDocument;

    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_issue_time(value: &str) -> bool {
    (20..=40).contains(&value.len())
        && value.ends_with('Z')
        && value.contains('T')
        && !value.bytes().any(|byte| byte.is_ascii_whitespace())
}

fn crosslink_key_matches(actor: &ActorEntry, provenance: &ActorProvenanceEntry) -> bool {
    let Some(public_key) = provenance.public_key.as_deref() else { return false };
    let Some(allowed_signer) = provenance.allowed_signer.as_deref() else { return false };
    let public: Vec<_> = public_key.split_ascii_whitespace().collect();
    let signer: Vec<_> = allowed_signer.split_ascii_whitespace().collect();
    if public.len() != 3
        || signer.len() != 4
        || public[0] != "ssh-ed25519"
        || signer[0] != "6ME5@crosslink"
        || signer[1] != public[0]
        || signer[2] != public[1]
        || signer[3] != public[2]
    {
        return false;
    }
    let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(public[1]) else {
        return false;
    };
    let digest = Sha256::digest(blob);
    let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest);
    actor.principal == format!("SHA256:{encoded}")
}

const fn actor_subject(actor: &ActorEntry) -> (ActorKind, &str) {
    (actor.kind, actor.principal.as_str())
}

fn valid_principal(kind: ActorKind, principal: &str) -> bool {
    match kind {
        ActorKind::CrosslinkAgent => principal.strip_prefix("SHA256:").is_some_and(|digest| {
            digest.len() == 43
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        }),
        ActorKind::CodexSubagent => !principal.is_empty() && principal.is_ascii(),
    }
}
