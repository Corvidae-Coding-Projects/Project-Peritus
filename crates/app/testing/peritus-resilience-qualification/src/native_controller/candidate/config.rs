//! Private staged-daemon configuration and file creation.

use std::fs;
use std::path::Path;

use super::process::create_output;

pub(super) fn render_configuration(state: &Path, registry: &Path, build_sha256: &str) -> String {
    format!(
        "version = 1\nstore_id = \"{}\"\n\n[paths]\nstate_root = {}\nartifact_root = {}\nevidence_root = {}\nworkspace_root = {}\nprocess_root = {}\ntransaction_root = {}\nbackup_root = {}\n\n[approval_registry]\npayload_file = {}\ngeneration = 1\n\n[human]\nactor_id = \"{}\"\n\n[product]\nautomatic_provider_failover = false\n\n[telemetry]\nmode = \"disabled\"\n\n[tools]\nallow = []\n",
        &build_sha256[..32],
        toml_path(state),
        toml_path(&state.join("artifacts")),
        toml_path(&state.join("evidence")),
        toml_path(&state.join("workspaces")),
        toml_path(&state.join("processes")),
        toml_path(&state.join("transactions")),
        toml_path(&state.join("backups")),
        toml_path(registry),
        "42".repeat(16),
    )
}

pub(super) fn bytes_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    use std::io::Write as _;
    let mut file = create_output(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(super) fn create_private_directory(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    builder.create(path)
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}
