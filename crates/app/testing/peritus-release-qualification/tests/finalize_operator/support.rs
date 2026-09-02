//! Shared signed-evidence fixture for the final H4 operator tests.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use ed25519_dalek::{Signer, SigningKey};
use peritus_release_artifacts::{
    ArtifactEntry, ArtifactInventory, ArtifactRole, BoundedId, BuildWitness, CandidateCommit,
    Ed25519PublicKey, Ed25519Signature, MediaType, PlatformTriple, ReleaseBinding, ReleasePath,
    ReleaseVersion, ToolchainId, compare_builds, digest_bytes,
};
use peritus_release_qualification::{
    AcceptanceCriterion, AuditDraft, CriterionEvidenceMap, CriterionMapping, EvidenceDisposition,
    EvidenceKind, EvidenceManifest, EvidenceManifestEntry, EvidenceManifestRole, EvidenceSignature,
    ParticipantId, SignedEvidenceRecord, canonical_evidence_signature_envelope,
};
use serde_json::{Value, json};

pub fn prepare_fixture(root: &Path) -> PathBuf {
    for directory in ["payloads", "keys", "signatures", "builds/primary", "builds/independent"] {
        fs::create_dir_all(root.join(directory)).expect("fixture directory");
    }
    fs::write(root.join("builds/primary/peritus"), b"release-binary").expect("primary artifact");
    fs::write(root.join("builds/independent/peritus"), b"release-binary")
        .expect("independent artifact");
    let binding = binding();
    let inventory = inventory(&binding, b"release-binary");
    let comparison = comparison(&inventory);
    let (mut specs, mut admitted) = evidence_set(root, &binding, &inventory, &comparison);
    let qualification_kinds = qualification_kinds();
    let criterion_map = criterion_map(&admitted, qualification_kinds);
    let criterion_record = sign_evidence(
        root,
        &binding,
        EvidenceKind::CriterionMap,
        &criterion_map.canonical_json().expect("criterion JSON"),
        91,
    );
    specs.push(criterion_record.spec);
    admitted.insert(EvidenceKind::CriterionMap, criterion_record.record);
    let audit_spec = prepare_audit(root, &binding, &admitted);
    let plan = plan_json(&binding, &specs, qualification_kinds, &audit_spec);
    let plan_path = root.join("plan.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).expect("plan JSON")).expect("plan");
    plan_path
}

fn comparison(
    inventory: &ArtifactInventory,
) -> peritus_release_artifacts::ReproducibilityComparison {
    let first = BuildWitness::from_inventory(
        BoundedId::new("builder-primary").expect("builder"),
        inventory,
    )
    .expect("first witness");
    let second = BuildWitness::from_inventory(
        BoundedId::new("builder-independent").expect("builder"),
        inventory,
    )
    .expect("second witness");
    compare_builds(&first, &second).expect("comparison")
}

fn evidence_set(
    root: &Path,
    binding: &ReleaseBinding,
    inventory: &ArtifactInventory,
    comparison: &peritus_release_artifacts::ReproducibilityComparison,
) -> (Vec<Value>, BTreeMap<EvidenceKind, SignedEvidenceRecord>) {
    let mut admitted = BTreeMap::new();
    let mut specs = Vec::new();
    for (index, kind) in EvidenceKind::required_signed_inputs()
        .into_iter()
        .filter(|kind| *kind != EvidenceKind::CriterionMap)
        .enumerate()
    {
        let payload = match kind {
            EvidenceKind::ArtifactInventory => inventory.canonical_json().expect("inventory JSON"),
            EvidenceKind::Reproducibility => comparison.canonical_json().expect("comparison JSON"),
            kind if EvidenceKind::fresh_subject_campaigns().contains(&kind) => {
                serde_json::to_vec(&campaign_json(kind)).expect("campaign JSON")
            }
            _ => serde_json::to_vec(&json!({
                "kind": kind,
                "verdict": "ready",
                "candidate": binding.candidate_commit().as_str(),
            }))
            .expect("evidence JSON"),
        };
        let signed = sign_evidence(root, binding, kind, &payload, u8_seed(index + 1));
        specs.push(signed.spec);
        admitted.insert(kind, signed.record);
    }
    (specs, admitted)
}

const fn qualification_kinds() -> [EvidenceKind; 6] {
    [
        EvidenceKind::H0SecurityReport,
        EvidenceKind::H1ResilienceReport,
        EvidenceKind::H2LinuxReport,
        EvidenceKind::H2MacosReport,
        EvidenceKind::H2WindowsReport,
        EvidenceKind::H3PerformanceReport,
    ]
}

fn criterion_map(
    admitted: &BTreeMap<EvidenceKind, SignedEvidenceRecord>,
    qualification_kinds: [EvidenceKind; 6],
) -> CriterionEvidenceMap {
    let qualification_references = qualification_kinds
        .iter()
        .map(|kind| {
            admitted.get(kind).expect("qualification reference").evidence_reference().clone()
        })
        .collect::<Vec<_>>();
    let mappings = AcceptanceCriterion::all()
        .into_iter()
        .map(|criterion| {
            CriterionMapping::new(criterion, qualification_references.clone())
                .expect("criterion mapping")
        })
        .collect();
    CriterionEvidenceMap::new(mappings).expect("criterion map")
}

fn prepare_audit(
    root: &Path,
    binding: &ReleaseBinding,
    admitted: &BTreeMap<EvidenceKind, SignedEvidenceRecord>,
) -> Value {
    let pre_audit = EvidenceManifest::new(
        binding.clone(),
        admitted
            .values()
            .map(|record| {
                EvidenceManifestEntry::from_reference(
                    role(record.evidence_reference().kind()),
                    record.evidence_reference(),
                )
                .expect("manifest entry")
            })
            .collect(),
    )
    .expect("pre-audit manifest");
    let audit = AuditDraft::new(
        binding.clone(),
        ParticipantId::new("independent-final-auditor").expect("auditor"),
        vec![ParticipantId::new("release-producer").expect("producer")],
        pre_audit.pre_audit_digest().expect("pre-audit digest"),
        Vec::new(),
    )
    .expect("audit draft");
    sign_audit(root, binding, &audit)
}

fn plan_json(
    binding: &ReleaseBinding,
    specs: &[Value],
    qualification_kinds: [EvidenceKind; 6],
    audit_spec: &Value,
) -> Value {
    let campaigns =
        EvidenceKind::fresh_subject_campaigns().into_iter().map(campaign_json).collect::<Vec<_>>();
    let criteria = AcceptanceCriterion::all()
        .into_iter()
        .map(|criterion| {
            json!({
                "criterion": criterion.id(),
                "evidence": qualification_kinds.map(|kind| json!({
                    "kind": kind,
                    "path": retained_path(kind)
                }))
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": 1,
        "binding": binding_json(binding),
        "evidence": specs,
        "campaigns": campaigns,
        "primary_build": build_json("builder-primary", "builds/primary/peritus"),
        "independent_build": build_json("builder-independent", "builds/independent/peritus"),
        "criteria": criteria,
        "audit": audit_spec,
        "evaluated_at": 50
    })
}

fn campaign_json(kind: EvidenceKind) -> Value {
    let index = EvidenceKind::fresh_subject_campaigns()
        .iter()
        .position(|candidate| *candidate == kind)
        .expect("campaign kind");
    json!({
        "schema_version": 1,
        "kind": kind,
        "subject_id": format!("fresh-h4-subject-{}", index + 1),
        "cleanup": {
            "remaining_processes": 0,
            "remaining_mounts": 0,
            "remaining_worktrees": 0,
            "remaining_temporary_paths": 0
        }
    })
}

struct SignedFixture {
    spec: Value,
    record: SignedEvidenceRecord,
}

fn sign_evidence(
    root: &Path,
    binding: &ReleaseBinding,
    kind: EvidenceKind,
    payload: &[u8],
    seed: u8,
) -> SignedFixture {
    let slug = kind_slug(kind);
    let path = retained_path(kind);
    let key_path = format!("keys/{slug}.pub");
    let signature_path = format!("signatures/{slug}.sig");
    fs::write(root.join(&path), payload).expect("evidence payload");
    let signing = SigningKey::from_bytes(&[seed.max(1); 32]);
    let release_path = ReleasePath::new(&path).expect("release path");
    let envelope = canonical_evidence_signature_envelope(
        binding,
        kind,
        EvidenceDisposition::Satisfied,
        &release_path,
        payload,
    )
    .expect("envelope");
    let signature = signing.sign(&envelope).to_bytes();
    fs::write(root.join(&key_path), signing.verifying_key().to_bytes()).expect("public key");
    fs::write(root.join(&signature_path), signature).expect("signature");
    let key_id = format!("signer-{seed:03}");
    let record = SignedEvidenceRecord::verify(
        binding.clone(),
        kind,
        EvidenceDisposition::Satisfied,
        release_path,
        payload,
        EvidenceSignature::new(
            BoundedId::new(&key_id).expect("key ID"),
            Ed25519PublicKey::from_bytes(signing.verifying_key().to_bytes()),
            Ed25519Signature::from_bytes(signature),
        ),
    )
    .expect("verified record");
    SignedFixture {
        spec: json!({
            "kind": kind,
            "disposition": "satisfied",
            "path": path,
            "key_id": key_id,
            "public_key_path": key_path,
            "signature_path": signature_path
        }),
        record,
    }
}

fn sign_audit(root: &Path, binding: &ReleaseBinding, audit: &AuditDraft) -> Value {
    let payload = audit.canonical_json().expect("audit JSON");
    let path = "payloads/final-audit.json";
    let key_path = "keys/final-audit.pub";
    let signature_path = "signatures/final-audit.sig";
    fs::write(root.join(path), &payload).expect("audit payload");
    let signing = SigningKey::from_bytes(&[201_u8; 32]);
    let release_path = ReleasePath::new(path).expect("audit path");
    let envelope = canonical_evidence_signature_envelope(
        binding,
        EvidenceKind::FinalAudit,
        EvidenceDisposition::Satisfied,
        &release_path,
        &payload,
    )
    .expect("audit envelope");
    fs::write(root.join(key_path), signing.verifying_key().to_bytes()).expect("audit key");
    fs::write(root.join(signature_path), signing.sign(&envelope).to_bytes())
        .expect("audit signature");
    json!({
        "auditor": "independent-final-auditor",
        "contributors": ["release-producer"],
        "findings": [],
        "path": path,
        "key_id": "final-auditor-key",
        "public_key_path": key_path,
        "signature_path": signature_path
    })
}

fn binding() -> ReleaseBinding {
    ReleaseBinding::new(
        CandidateCommit::new("42".repeat(20)).expect("commit"),
        ReleaseVersion::new("1.0.0").expect("version"),
        ToolchainId::new("rust-1.97.1_verus-0.2026.08.09").expect("toolchain"),
        PlatformTriple::new("tier-one-linux-macos-windows").expect("platform"),
        digest_bytes(b"exact-source-tree"),
    )
}

fn inventory(binding: &ReleaseBinding, bytes: &[u8]) -> ArtifactInventory {
    ArtifactInventory::new(
        binding.clone(),
        vec![
            ArtifactEntry::from_bytes(
                ReleasePath::new("peritus").expect("path"),
                MediaType::new("application/octet-stream").expect("media"),
                vec![ArtifactRole::Distribution, ArtifactRole::Executable],
                bytes,
            )
            .expect("artifact"),
        ],
    )
    .expect("inventory")
}

fn binding_json(binding: &ReleaseBinding) -> Value {
    json!({
        "candidate_commit": binding.candidate_commit().as_str(),
        "version": binding.version().as_str(),
        "toolchain": binding.toolchain().as_str(),
        "platform": binding.platform().as_str(),
        "source_tree_digest": binding.source_tree_digest()
    })
}

fn build_json(builder: &str, source: &str) -> Value {
    json!({
        "builder_id": builder,
        "artifacts": [{
            "path": "peritus",
            "source_path": source,
            "media_type": "application/octet-stream",
            "roles": ["distribution", "executable"]
        }]
    })
}

pub fn finalize(plan: &Path, evidence_root: &Path, output: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_peritus-h4"));
    command
        .arg("finalize")
        .args(["--plan", plan.to_str().expect("plan path")])
        .args(["--evidence-root", evidence_root.to_str().expect("evidence root")])
        .args(["--output", output.to_str().expect("output path")]);
    command
}

fn retained_path(kind: EvidenceKind) -> String {
    format!("payloads/{}.json", kind_slug(kind))
}

fn kind_slug(kind: EvidenceKind) -> String {
    serde_json::to_value(kind).expect("kind JSON").as_str().expect("kind string").to_owned()
}

fn u8_seed(index: usize) -> u8 {
    u8::try_from(index).expect("bounded evidence catalog").saturating_add(1)
}

const fn role(kind: EvidenceKind) -> EvidenceManifestRole {
    match kind {
        EvidenceKind::H0SecurityReport => EvidenceManifestRole::H0SecurityReport,
        EvidenceKind::H1ResilienceReport => EvidenceManifestRole::H1ResilienceReport,
        EvidenceKind::H2LinuxReport => EvidenceManifestRole::H2LinuxReport,
        EvidenceKind::H2MacosReport => EvidenceManifestRole::H2MacosReport,
        EvidenceKind::H2WindowsReport => EvidenceManifestRole::H2WindowsReport,
        EvidenceKind::H3PerformanceReport => EvidenceManifestRole::H3PerformanceReport,
        EvidenceKind::GateA => EvidenceManifestRole::GateA,
        EvidenceKind::Foundation => EvidenceManifestRole::Foundation,
        EvidenceKind::NativeLinux => EvidenceManifestRole::NativeLinux,
        EvidenceKind::NativeMacos => EvidenceManifestRole::NativeMacos,
        EvidenceKind::NativeWindows => EvidenceManifestRole::NativeWindows,
        EvidenceKind::Soak => EvidenceManifestRole::Soak,
        EvidenceKind::RepresentativeRust => EvidenceManifestRole::RepresentativeRust,
        EvidenceKind::RepresentativeTypeScript => EvidenceManifestRole::RepresentativeTypeScript,
        EvidenceKind::RepresentativePython => EvidenceManifestRole::RepresentativePython,
        EvidenceKind::RepresentativeJava => EvidenceManifestRole::RepresentativeJava,
        EvidenceKind::RepresentativeMixed => EvidenceManifestRole::RepresentativeMixed,
        EvidenceKind::ArtifactInventory => EvidenceManifestRole::ArtifactInventory,
        EvidenceKind::SpdxSbom => EvidenceManifestRole::SpdxSbom,
        EvidenceKind::Provenance => EvidenceManifestRole::Provenance,
        EvidenceKind::ArtifactSignatures => EvidenceManifestRole::ArtifactSignatures,
        EvidenceKind::Reproducibility => EvidenceManifestRole::Reproducibility,
        EvidenceKind::MigrationRecovery => EvidenceManifestRole::MigrationRecovery,
        EvidenceKind::LicenseNotices => EvidenceManifestRole::LicenseNotices,
        EvidenceKind::CriterionMap => EvidenceManifestRole::CriterionMap,
        EvidenceKind::FinalAudit => EvidenceManifestRole::FinalAudit,
    }
}
