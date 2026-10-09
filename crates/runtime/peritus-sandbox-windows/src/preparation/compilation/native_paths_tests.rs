//! Real filesystem regression for configured roots before ACL projection or authority use.

use super::WindowsBackend;
use crate::{ProbeEvidence, TokenProfile, WindowsBackendConfig, WindowsPath, WindowsProbe};
use std::io::Write as _;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::Path;

fn backend(workspace: &WindowsPath, protected: WindowsPath, backup: &Path) -> WindowsBackend {
    let config = WindowsBackendConfig::new(
        std::env::current_exe().unwrap(),
        workspace.clone(),
        vec![protected],
        backup.to_path_buf(),
        TokenProfile::restricted("S-1-1-0").unwrap(),
        None,
        None,
        None,
    )
    .unwrap();
    // Probe construction here is inert; only the actual configured-path validator runs.
    let mut evidence = ProbeEvidence::unsupported();
    evidence.helper = true;
    evidence.helper_digest = Some(peritus_codec::sha256(b"inert path-validation fixture"));
    WindowsBackend::from_probe(config, WindowsProbe::from_evidence(evidence).unwrap()).unwrap()
}

#[test]
fn canonical_protected_roots_allow_existing_or_only_missing_final_component() {
    let root = tempfile::tempdir().unwrap();
    let workspace =
        WindowsPath::from_canonicalized(&std::fs::canonicalize(root.path()).unwrap()).unwrap();
    let native = workspace.to_path_buf();
    let existing = native.join("existing");
    let missing = native.join("missing");
    let backup = native.join("backup");
    std::fs::create_dir(&existing).unwrap();
    for accepted in [&existing, &missing] {
        let protected = WindowsPath::from_os_str(accepted.as_os_str()).unwrap();
        backend(&workspace, protected, &backup).validate_configured_native_paths().unwrap();
    }
    let inaccessible = WindowsPath::from_os_str(missing.join("child").as_os_str()).unwrap();
    assert!(backend(&workspace, inaccessible, &backup).validate_configured_native_paths().is_err());
    assert!(existing.is_dir());
    assert!(!missing.exists(), "path validation must not create a deny anchor");
    assert!(!backup.exists(), "path validation must not begin an ACL transaction");
}

#[test]
fn native_short_name_protected_roots_are_rejected_before_acl_projection() {
    let root = tempfile::tempdir().unwrap();
    let workspace_native = root.path().join("Peritus protected path identity regression");
    std::fs::create_dir(&workspace_native).unwrap();
    let workspace =
        WindowsPath::from_canonicalized(&std::fs::canonicalize(&workspace_native).unwrap())
            .unwrap();
    let native = workspace.to_path_buf();
    let short = short_path(&native);
    let short_identity = WindowsPath::from_os_str(short.as_os_str()).unwrap();
    if workspace.same_native_path(&short_identity) {
        report_alias_coverage("Windows 8.3 alias coverage: unavailable (no distinct OS spelling)");
        return;
    }
    assert_eq!(std::fs::canonicalize(&short).unwrap(), std::fs::canonicalize(&native).unwrap());
    let existing = native.join("existing");
    std::fs::create_dir(&existing).unwrap();
    let backup = native.join("backup");
    // Lexical projection would omit these roots, despite their physical workspace overlap.
    for name in ["existing", "missing"] {
        let protected = WindowsPath::from_os_str(short.join(name).as_os_str()).unwrap();
        assert!(!workspace.contains(&protected));
        let error = backend(&workspace, protected, &backup)
            .validate_configured_native_paths()
            .expect_err("a noncanonical protected root must fail before lexical projection");
        assert!(error.to_string().contains("resolved path identity differs"), "{error}");
    }
    assert!(existing.is_dir());
    assert!(!native.join("missing").exists(), "rejection must not create a deny anchor");
    assert!(!backup.exists(), "rejection must not begin an ACL transaction");
    report_alias_coverage("Windows 8.3 alias coverage: rejection exercised for 2 cases");
}

fn report_alias_coverage(status: &str) {
    // Direct output bypasses passing-test capture so hosted logs retain conditional coverage.
    writeln!(std::io::stdout().lock(), "{status}").expect("write native alias coverage status");
}

#[allow(unsafe_code, reason = "bounded native short-path query for real alias regression")]
fn short_path(path: &Path) -> std::path::PathBuf {
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let input = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: input is NUL-terminated and live; a null output with zero capacity queries length.
    let required = unsafe { GetShortPathNameW(input.as_ptr(), core::ptr::null_mut(), 0) };
    assert_ne!(required, 0, "GetShortPathNameW size query: {}", std::io::Error::last_os_error());
    let mut output = vec![0u16; required as usize];
    // SAFETY: input remains live and output contains exactly the advertised writable capacity.
    let written = unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), required) };
    assert!(
        written > 0 && written < required,
        "GetShortPathNameW fill: {}",
        std::io::Error::last_os_error()
    );
    output.truncate(written as usize);
    std::ffi::OsString::from_wide(&output).into()
}
