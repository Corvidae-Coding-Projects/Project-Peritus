//! Integration tests for reproducible evidence-manifest content addressing.

use peritus_benchmarks::{
    ArtifactPath, EvidenceArtifact, EvidenceManifestBuilder, ReferenceMachine, RunnerDescriptor,
    Sha256Digest, StableId, SubjectDescriptor,
};

#[test]
fn artifact_order_does_not_change_manifest_digest() {
    let machine = ReferenceMachine::new(
        StableId::new("linux").expect("id"),
        StableId::new("x86_64").expect("id"),
        "test cpu",
        4,
        4096,
        StableId::new("test-disk").expect("id"),
    )
    .expect("machine");
    let subject = SubjectDescriptor::new(
        StableId::new("peritus-daemon").expect("id"),
        "revision",
        Sha256Digest::of_bytes(b"subject"),
    )
    .expect("subject");
    let runner = RunnerDescriptor::new(
        StableId::new("runner").expect("id"),
        "1",
        Sha256Digest::of_bytes(b"runner"),
    )
    .expect("runner");
    let first = EvidenceArtifact::from_bytes(
        ArtifactPath::new("measurements/a.jsonl").expect("path"),
        "application/x-ndjson",
        b"a",
    )
    .expect("artifact");
    let second = EvidenceArtifact::from_bytes(
        ArtifactPath::new("measurements/b.jsonl").expect("path"),
        "application/x-ndjson",
        b"b",
    )
    .expect("artifact");
    let build = |artifacts: [EvidenceArtifact; 2]| {
        let mut builder = EvidenceManifestBuilder::new(
            StableId::new("run").expect("id"),
            StableId::new("profile").expect("id"),
            subject.clone(),
            runner.clone(),
            machine.clone(),
        )
        .dataset_digests(Sha256Digest::of_bytes(b"profile"), Sha256Digest::of_bytes(b"workloads"))
        .time_range(1, 2)
        .expect("time")
        .measurement_count(2);
        for artifact in artifacts {
            builder = builder.artifact(artifact);
        }
        builder.build().expect("manifest")
    };
    let forward = build([first.clone(), second.clone()]);
    let reverse = build([second, first]);
    assert_eq!(forward.digest().expect("digest"), reverse.digest().expect("digest"));
}
