//! Immutable guest image overlay, cloud-init seed, and candidate payload media.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::config::{create_private_directory, render_configuration, write_new};
use crate::native_controller::args::ControllerPaths;

pub(super) struct GuestMedia {
    pub(super) overlay: PathBuf,
    pub(super) seed_iso: PathBuf,
    pub(super) payload_iso: PathBuf,
    pub(super) private_key: PathBuf,
}

pub(super) fn create(
    paths: &ControllerPaths,
    root: &Path,
) -> Result<GuestMedia, Box<dyn std::error::Error>> {
    let base = paths
        .controller_resource
        .as_deref()
        .ok_or("host reboot qualification requires a bound guest image")?;
    let media_root = root.join("reboot-media");
    create_private_directory(&media_root)?;
    let overlay = media_root.join("guest-overlay.qcow2");
    checked(
        Command::new("qemu-img")
            .arg("create")
            .arg("-q")
            .arg("-f")
            .arg("qcow2")
            .arg("-F")
            .arg("qcow2")
            .arg("-b")
            .arg(base)
            .arg(&overlay),
        "create guest copy-on-write overlay",
    )?;

    let private_key = media_root.join("guest-key");
    checked(
        Command::new("ssh-keygen")
            .arg("-q")
            .arg("-t")
            .arg("ed25519")
            .arg("-N")
            .arg("")
            .arg("-f")
            .arg(&private_key),
        "create disposable guest SSH key",
    )?;
    let public_key_document = fs::read_to_string(private_key.with_extension("pub"))?;
    let public_key = one_line(&public_key_document, "guest public key")?;
    if !public_key.starts_with("ssh-ed25519 ") {
        return Err("disposable guest key is not Ed25519".into());
    }

    let seed = media_root.join("cloud-init");
    create_private_directory(&seed)?;
    write_new(
        &seed.join("meta-data"),
        format!("instance-id: {}\nlocal-hostname: peritus-h1\n", paths.instance_id).as_bytes(),
    )?;
    write_new(&seed.join("user-data"), cloud_config(public_key).as_bytes())?;
    write_new(&seed.join("network-config"), network_config().as_bytes())?;
    let seed_iso = media_root.join("cloud-init.iso");
    iso(&seed, &seed_iso, "cidata", &["meta-data", "user-data", "network-config"])?;

    let payload = media_root.join("payload");
    create_private_directory(&payload)?;
    fs::copy(&paths.candidate, payload.join("peritusd"))?;
    let guest_state = Path::new("/var/lib/peritus-h1/state");
    let guest_registry = Path::new("/var/lib/peritus-h1/approval-registry.bin");
    write_new(
        &payload.join("peritus.toml"),
        render_configuration(guest_state, guest_registry, &paths.build_sha256).as_bytes(),
    )?;
    fs::copy(root.join("approval-registry.bin"), payload.join("approval-registry.bin"))?;
    let payload_iso = media_root.join("payload.iso");
    iso(
        &payload,
        &payload_iso,
        "PERITUS_H1",
        &["peritusd", "peritus.toml", "approval-registry.bin"],
    )?;
    Ok(GuestMedia { overlay, seed_iso, payload_iso, private_key })
}

fn cloud_config(public_key: &str) -> String {
    // Alpine locks root at the OS layer by default. The hash unlocks that account so OpenSSH can
    // accept its disposable public key; password authentication remains disabled below.
    format!(
        "#cloud-config\nusers:\n  - name: root\n    lock_passwd: false\n    hashed_passwd: '$6$peritush1$goToYm2nUA8bjNq3kCSZEZLQEzihI19nqlJAajAdpa19sI5J.Kzr1D9lp4eNDo1FhDnWYpYwWPDsbIfSbuUeP0'\n    ssh_authorized_keys:\n      - {public_key}\nssh_pwauth: false\ndisable_root: false\nruncmd:\n  - [mkdir, -p, /mnt/peritus-payload]\n  - [mkdir, -p, /var/lib/peritus-h1/state]\n  - [mount, -o, ro, 'LABEL=PERITUS_H1', /mnt/peritus-payload]\n  - [install, -m, '0755', /mnt/peritus-payload/peritusd, /usr/local/bin/peritusd]\n  - [install, -m, '0644', /mnt/peritus-payload/peritus.toml, /var/lib/peritus-h1/peritus.toml]\n  - [install, -m, '0600', /mnt/peritus-payload/approval-registry.bin, /var/lib/peritus-h1/approval-registry.bin]\n  - [touch, /var/lib/peritus-h1/ready]\n"
    )
}

const fn network_config() -> &'static str {
    "version: 1\nconfig:\n  - type: physical\n    name: eth0\n    subnets:\n      - type: dhcp4\n"
}

fn iso(
    source: &Path,
    destination: &Path,
    label: &str,
    files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::new("mkisofs");
    command
        .arg("-quiet")
        .arg("-output")
        .arg(destination)
        .arg("-volid")
        .arg(label)
        .arg("-joliet")
        .arg("-rock")
        .current_dir(source);
    for file in files {
        command.arg(file);
    }
    checked(&mut command, "create guest ISO")
}

fn checked(command: &mut Command, operation: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{operation} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn one_line<'a>(value: &'a str, label: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        Err(format!("{label} is not one line").into())
    } else {
        Ok(value)
    }
}
