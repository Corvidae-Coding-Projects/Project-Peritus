//! Exact authority and receipt digests captured from pre-L006 commit 1ad514009354a29471ef0cb83ba1a5c56a4ee571.

use peritus_patch::{
    DirectoryMode, FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, WorkspacePath,
    apply_patch,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use std::fmt::Write as _;

#[test]
fn previously_accepted_inline_and_snapshot_authority_and_receipts_are_unchanged() {
    let fixtures = [
        (
            false,
            false,
            "08939475fb086e6d74c7cfbb35a3b1d1961f7231447eb219014190a75163a80a",
            "951d71132ca59df648ad0d5314768fdf07a81e413e0a24995bdb3e153dec6e9f",
        ),
        (
            false,
            true,
            "1e291354b9d526e4e17ed08ca2aed99fdce2dbccb8842ec9e824904bea0bd55c",
            "289fa168085aafb9484df4da46c70edebb5cf19fcbada3875255b81581c5fa46",
        ),
        (
            true,
            false,
            "b2cec0e3b924d473e301f2c85b4962bcbf468db031c4fe319314aa62ccca2444",
            "d2bc5697b84665a50500760e99265d7a27cea4df8eeaf93560db06559ab5e71e",
        ),
        (
            true,
            true,
            "9ce171bd4b8358c829c17fd20606f08545274a3cb59c0ece5cbcced8bc24daed",
            "59c5156ebc90ebdb53cf7f2b7a26996195a34a1cbe76fbc2b3cec1185e2a2946",
        ),
    ];
    for (directory, snapshot, expected_patch, expected_manifest) in fixtures {
        let workspace = tempfile::tempdir().unwrap();
        let transactions = tempfile::tempdir().unwrap();
        let operation = if directory {
            PatchOperation::create_directory(
                WorkspacePath::new("empty").unwrap(),
                DirectoryMode::new(0o777).unwrap(),
            )
        } else {
            PatchOperation::create(
                WorkspacePath::new("saved").unwrap(),
                FinalFile::new(b"saved".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
                    .unwrap(),
            )
        };
        let id = WorkspaceId::new([41; 16]).unwrap();
        let constructor = if snapshot { PatchSet::from_snapshot } else { PatchSet::new };
        let patch =
            constructor(id, Generation::first(), RevisionNumber::first(), vec![operation]).unwrap();
        let plan = patch.plan(id, Generation::first(), RevisionNumber::first()).unwrap();
        let applied = apply_patch(workspace.path(), transactions.path(), &plan).unwrap();
        let mut manifest_hex = String::new();
        for byte in applied.manifest_digest().as_bytes() {
            write!(&mut manifest_hex, "{byte:02x}").unwrap();
        }
        assert_eq!(applied.identity().to_hex(), expected_patch);
        assert_eq!(manifest_hex, expected_manifest);
    }
}
