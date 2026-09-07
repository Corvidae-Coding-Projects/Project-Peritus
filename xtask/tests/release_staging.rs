#![doc = "Black-box proof that release staging never requests public publication."]
#![cfg(unix)]

use std::{
    env,
    fmt::Write as _,
    fs,
    os::unix::fs::PermissionsExt as _,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;
use sha2::{Digest as _, Sha256};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = env::temp_dir().join(format!(
            "peritus-release-staging-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("new private fixture");
        fs::create_dir(root.join("commands")).expect("command directory");
        fs::write(root.join("Cargo.toml"), "[workspace.package]\nversion = \"1.2.3\"\n")
            .expect("manifest");
        fs::write(root.join("architecture.toml"), "").expect("workspace identity");
        for name in ["install.sh", "install.ps1"] {
            fs::write(root.join(name), "version='@PERITUS_RELEASE_TAG@'\n").expect("bootstrap");
        }
        let script = root.join("commands/gh");
        fs::write(
            &script,
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$PERITUS_STAGING_FIXTURE/calls"
case "$1 $2" in
  'release view')
    if test -f "$PERITUS_STAGING_FIXTURE/uploaded"; then
      /bin/cat "$PERITUS_STAGING_FIXTURE/after.json"
    else
      /bin/cat "$PERITUS_STAGING_FIXTURE/before.json"
    fi
    ;;
  'release upload')
    if test "$PERITUS_STAGING_UPLOAD_FAILURE" = 1; then exit 72; fi
    : > "$PERITUS_STAGING_FIXTURE/uploaded"
    ;;
  *) exit 73 ;;
esac
"#,
        )
        .expect("isolated GitHub transport");
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).expect("executable");
        let fixture = Self { root };
        fixture.release("before.json", true, false);
        fixture.release("after.json", true, true);
        fixture
    }

    fn release(&self, name: &str, draft: bool, bootstraps: bool) {
        let mut assets = Vec::new();
        for (platform, architecture) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
            ("windows", "aarch64"),
        ] {
            let extension = if platform == "windows" { "zip" } else { "tar.gz" };
            for suffix in [
                "",
                ".sha256",
                ".inventory.json",
                ".spdx.json",
                ".provenance.json",
                ".provenance.sigstore.jsonl",
                ".sbom.sigstore.jsonl",
                ".evidence.sha256",
            ] {
                assets.push(format!("peritus-{platform}-{architecture}.{extension}{suffix}"));
            }
        }
        for (architecture, debian) in [("x86_64", "amd64"), ("aarch64", "arm64")] {
            for format in ["deb", "rpm"] {
                for suffix in ["", ".asc", ".sha256"] {
                    assets.push(format!("peritus-{format}-{architecture}.tar.gz{suffix}"));
                }
            }
            assets.push(format!("peritus_1.2.3-1_{debian}.deb"));
            assets.push(format!("peritus-1.2.3-1.fc44.{architecture}.rpm"));
        }
        assets.push("peritus-release.asc".to_owned());
        if bootstraps {
            for name in ["install.sh", "install.ps1"] {
                assets.extend([name.to_owned(), format!("{name}.sha256")]);
            }
        }
        let assets = assets
            .into_iter()
            .map(|name| json!({"name":name,"size":1,"state":"uploaded"}))
            .collect::<Vec<_>>();
        fs::write(
            self.root.join(name),
            serde_json::to_vec(&json!({
                "tagName":"v1.2.3", "isDraft":draft, "assets":assets
            }))
            .expect("release JSON"),
        )
        .expect("transport response");
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .current_dir(&self.root)
            .arg("release-stage")
            .env("PATH", self.root.join("commands"))
            .env("GITHUB_REF_NAME", "v1.2.3")
            .env("PERITUS_STAGING_FIXTURE", &self.root)
            .env("PERITUS_STAGING_UPLOAD_FAILURE", "0");
        command
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("calls"))
            .expect("command log")
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove only this fixture");
    }
}

fn assert_failed(output: &Output) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("draft is complete"));
}

#[test]
fn complete_staging_only_inspects_uploads_and_reinspects_the_draft() {
    let fixture = Fixture::new();
    let output = fixture.command().output().expect("stage");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("remains unpublished"));
    let calls = fixture.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0], "release view v1.2.3 --json tagName,isDraft,assets");
    assert!(calls[1].starts_with("release upload v1.2.3 --clobber "));
    assert_eq!(calls[2], calls[0]);
    for name in ["install.sh", "install.ps1"] {
        let path = fixture.root.join("dist/bootstrap").join(name);
        let bytes = fs::read(&path).expect("staged bootstrap");
        assert_eq!(bytes, b"version='v1.2.3'\n");
        let checksum = fs::read_to_string(path.with_file_name(format!("{name}.sha256")))
            .expect("bootstrap checksum");
        let digest = Sha256::digest(&bytes);
        let mut hex = String::with_capacity(64);
        for byte in digest {
            write!(hex, "{byte:02x}").expect("format digest");
        }
        assert_eq!(checksum, format!("{hex}\n"));
    }
}

#[test]
fn a_published_release_is_rejected_before_any_upload() {
    let fixture = Fixture::new();
    fixture.release("before.json", false, true);
    assert_failed(&fixture.command().output().expect("stage"));
    assert_eq!(fixture.calls().len(), 1);
    assert!(!fixture.root.join("dist").exists());
}

#[test]
fn failed_upload_or_changed_draft_never_reports_completion() {
    let failure = Fixture::new();
    assert_failed(
        &failure.command().env("PERITUS_STAGING_UPLOAD_FAILURE", "1").output().expect("stage"),
    );
    assert_eq!(failure.calls().len(), 2);
    for (draft, bootstraps) in [(false, true), (true, false)] {
        let changed = Fixture::new();
        changed.release("after.json", draft, bootstraps);
        assert_failed(&changed.command().output().expect("stage"));
        assert_eq!(changed.calls().len(), 3);
    }
}

#[test]
fn wrong_version_cannot_reach_the_github_transport() {
    let fixture = Fixture::new();
    assert_failed(&fixture.command().env("GITHUB_REF_NAME", "v9.9.9").output().expect("stage"));
    assert!(!fixture.root.join("calls").exists());
}
