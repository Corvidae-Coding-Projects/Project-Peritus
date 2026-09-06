//! Complete target inventory and tag-bound public bootstrap publication.

use std::{fs, path::Path, process::Command};

use serde::Deserialize;

use crate::XtaskError;

const TARGETS: [(&str, &str, &str); 6] = [
    ("linux", "x86_64", "ubuntu-24.04"),
    ("linux", "aarch64", "ubuntu-24.04-arm"),
    ("macos", "aarch64", "macos-15"),
    ("macos", "x86_64", "macos-15-intel"),
    ("windows", "x86_64", "windows-2025"),
    ("windows", "aarch64", "windows-11-arm"),
];
const BOOTSTRAPS: [&str; 2] = ["install.sh", "install.ps1"];
const TAG_TOKEN: &str = "@PERITUS_RELEASE_TAG@";
const EVIDENCE_SUFFIXES: [&str; 8] = [
    "",
    ".sha256",
    ".inventory.json",
    ".spdx.json",
    ".provenance.json",
    ".provenance.sigstore.jsonl",
    ".sbom.sigstore.jsonl",
    ".evidence.sha256",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    tag_name: String,
    is_draft: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    size: u64,
    state: String,
}

pub(crate) fn publish(root: &Path) -> Result<(), XtaskError> {
    let tag = super::environment("GITHUB_REF_NAME")?;
    let expected_tag = format!("v{}", super::workspace_version(root)?);
    if tag != expected_tag {
        return Err(XtaskError::metadata("release tag does not match the workspace version"));
    }
    validate_release(&read_release(root, &tag)?, &tag, false)?;
    let bootstraps = stage_bootstraps(root, &tag)?;
    super::run(
        Command::new("gh")
            .current_dir(root)
            .args(["release", "upload", &tag, "--clobber"])
            .args(bootstraps),
        "upload tag-bound release installers and their checksums",
    )?;
    validate_release(&read_release(root, &tag)?, &tag, true)?;
    super::run(
        Command::new("gh").current_dir(root).args([
            "release",
            "edit",
            &tag,
            "--draft=false",
            "--latest",
        ]),
        "publish complete GitHub release",
    )
}

fn read_release(root: &Path, tag: &str) -> Result<Release, XtaskError> {
    let output = Command::new("gh")
        .current_dir(root)
        .args(["release", "view", tag, "--json", "tagName,isDraft,assets"])
        .output()
        .map_err(|error| XtaskError::io("inspect draft release from", root, error))?;
    if !output.status.success() {
        return Err(XtaskError::metadata("could not inspect the exact draft release"));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| XtaskError::metadata(format!("invalid GitHub release response: {error}")))
}

fn expected_assets(include_bootstraps: bool) -> Vec<String> {
    let mut names = Vec::new();
    for (platform, architecture, _) in TARGETS {
        let extension = if platform == "windows" { "zip" } else { "tar.gz" };
        let archive = format!("peritus-{platform}-{architecture}.{extension}");
        names.extend(EVIDENCE_SUFFIXES.iter().map(|suffix| format!("{archive}{suffix}")));
    }
    if include_bootstraps {
        for bootstrap in BOOTSTRAPS {
            names.push(bootstrap.to_owned());
            names.push(format!("{bootstrap}.sha256"));
        }
    }
    names
}

fn validate_release(
    release: &Release,
    tag: &str,
    include_bootstraps: bool,
) -> Result<(), XtaskError> {
    if !release.is_draft || release.tag_name != tag {
        return Err(XtaskError::metadata("publication requires the exact unpublished draft"));
    }
    for name in expected_assets(include_bootstraps) {
        let mut matching = release.assets.iter().filter(|asset| asset.name == name);
        if !matching.next().is_some_and(|asset| asset.state == "uploaded" && asset.size > 0)
            || matching.next().is_some()
        {
            return Err(XtaskError::metadata(format!(
                "release requires exactly one complete, nonempty asset: {name}"
            )));
        }
    }
    Ok(())
}

fn render_bootstrap(source: &str, tag: &str) -> Result<String, XtaskError> {
    let version = tag.strip_prefix('v').unwrap_or("");
    let components = version.split('.').collect::<Vec<_>>();
    if components.len() != 3
        || components
            .iter()
            .any(|value| value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()))
        || source.matches(TAG_TOKEN).count() != 1
    {
        return Err(XtaskError::metadata(
            "bootstrap requires one tag token and a vMAJOR.MINOR.PATCH version",
        ));
    }
    Ok(source.replace(TAG_TOKEN, tag))
}

fn stage_bootstraps(root: &Path, tag: &str) -> Result<Vec<std::path::PathBuf>, XtaskError> {
    let directory = root.join("dist/bootstrap");
    fs::create_dir_all(&directory).map_err(|error| {
        XtaskError::io("create release bootstrap directory at", &directory, error)
    })?;
    let mut assets = Vec::new();
    for name in BOOTSTRAPS {
        let source_path = root.join(name);
        let source = fs::read_to_string(&source_path)
            .map_err(|error| XtaskError::io("read release bootstrap at", &source_path, error))?;
        let rendered = render_bootstrap(&source, tag)?;
        let target = directory.join(name);
        fs::write(&target, rendered)
            .map_err(|error| XtaskError::io("write release bootstrap at", &target, error))?;
        let checksum = directory.join(format!("{name}.sha256"));
        fs::write(&checksum, format!("{}\n", super::digest(&target)?))
            .map_err(|error| XtaskError::io("write bootstrap checksum at", &checksum, error))?;
        assets.extend([target, checksum]);
    }
    Ok(assets)
}

#[cfg(test)]
mod tests;
