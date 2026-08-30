//! Black-box H4 envelope preparation and signature admission.

use std::{fs, path::Path, process::Command};

use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;

#[test]
fn operator_prepares_exact_envelope_and_admits_only_matching_signature() {
    let root = tempfile::tempdir().expect("operator root");
    let binding = root.path().join("binding.json");
    let payload = root.path().join("h0-report.json");
    let envelope = root.path().join("h0-envelope.json");
    let public_key = root.path().join("public-key");
    let signature = root.path().join("signature");
    let record = root.path().join("h0-record.json");
    write_binding(&binding);
    fs::write(&payload, br#"{"verdict":"ready"}"#).expect("payload");

    let prepared = command("envelope", &binding, &payload, &envelope).output().expect("envelope");
    assert!(prepared.status.success(), "{}", String::from_utf8_lossy(&prepared.stderr));
    let envelope_bytes = fs::read(&envelope).expect("envelope bytes");
    let signing_key = SigningKey::from_bytes(&[29_u8; 32]);
    fs::write(&public_key, signing_key.verifying_key().to_bytes()).expect("public key");
    fs::write(&signature, signing_key.sign(&envelope_bytes).to_bytes()).expect("signature");

    let verified = command("verify", &binding, &payload, &record)
        .args(["--key-id", "release-reviewer-1"])
        .args(["--public-key", public_key.to_str().expect("public key path")])
        .args(["--signature", signature.to_str().expect("signature path")])
        .output()
        .expect("verify");
    assert!(verified.status.success(), "{}", String::from_utf8_lossy(&verified.stderr));
    let admitted: Value =
        serde_json::from_slice(&fs::read(&record).expect("record bytes")).expect("record JSON");
    assert_eq!(admitted["reference"]["kind"], "h0-security-report");
    assert_eq!(admitted["reference"]["disposition"], "satisfied");
    assert_eq!(admitted["signature"]["algorithm"], "Ed25519");

    let overwrite = command("verify", &binding, &payload, &record)
        .args(["--key-id", "release-reviewer-1"])
        .args(["--public-key", public_key.to_str().expect("public key path")])
        .args(["--signature", signature.to_str().expect("signature path")])
        .output()
        .expect("overwrite");
    assert!(!overwrite.status.success());
}

#[test]
fn operator_rejects_payload_substitution_after_signing() {
    let root = tempfile::tempdir().expect("operator root");
    let binding = root.path().join("binding.json");
    let payload = root.path().join("h0-report.json");
    let envelope = root.path().join("h0-envelope.json");
    let public_key = root.path().join("public-key");
    let signature = root.path().join("signature");
    let record = root.path().join("h0-record.json");
    write_binding(&binding);
    fs::write(&payload, b"original").expect("payload");
    assert!(
        command("envelope", &binding, &payload, &envelope).status().expect("envelope").success()
    );
    let signing_key = SigningKey::from_bytes(&[31_u8; 32]);
    fs::write(&public_key, signing_key.verifying_key().to_bytes()).expect("public key");
    fs::write(
        &signature,
        signing_key.sign(&fs::read(&envelope).expect("envelope bytes")).to_bytes(),
    )
    .expect("signature");
    fs::write(&payload, b"substituted").expect("substitute payload");

    let output = command("verify", &binding, &payload, &record)
        .args(["--key-id", "release-reviewer-2"])
        .args(["--public-key", public_key.to_str().expect("public key path")])
        .args(["--signature", signature.to_str().expect("signature path")])
        .output()
        .expect("verify");
    assert!(!output.status.success());
    assert!(!record.exists());
}

fn command(command: &str, binding: &Path, payload: &Path, output: &Path) -> Command {
    let mut process = Command::new(env!("CARGO_BIN_EXE_peritus-h4"));
    process
        .arg(command)
        .args(["--binding", binding.to_str().expect("binding path")])
        .args(["--kind", "h0-security-report"])
        .args(["--disposition", "satisfied"])
        .args(["--retained-path", "qualification/h0-report.json"])
        .args(["--payload", payload.to_str().expect("payload path")])
        .args(["--output", output.to_str().expect("output path")]);
    process
}

fn write_binding(path: &Path) {
    let document = serde_json::json!({
        "candidate_commit": "42".repeat(20),
        "version": "1.0.0",
        "toolchain": "rust-1.97.1_verus-0.2026.08.09",
        "platform": "tier-one-linux-macos-windows",
        "source_tree_digest": "17".repeat(32),
    });
    fs::write(path, serde_json::to_vec(&document).expect("binding JSON")).expect("binding");
}
