//! Safe Landlock ABI probing and hard-requirement policy installation.

use crate::{
    HelperManifest, LandlockAccess, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery,
};
use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr, RulesetStatus,
};

/// Reports the greatest crate-known usable ABI without restricting the probing process.
#[must_use]
pub(super) fn probe_abi() -> Option<u8> {
    [
        (ABI::V9, 9),
        (ABI::V8, 8),
        (ABI::V7, 7),
        (ABI::V6, 6),
        (ABI::V5, 5),
        (ABI::V4, 4),
        (ABI::V3, 3),
        (ABI::V2, 2),
        (ABI::V1, 1),
    ]
    .into_iter()
    .find_map(|(abi, number)| {
        Ruleset::default()
            .handle_access(AccessFs::from_all(abi))
            .map(|ruleset| ruleset.set_compatibility(CompatLevel::HardRequirement))
            .and_then(Ruleset::create)
            .ok()
            .map(|_| number)
    })
}

pub(super) fn install(manifest: &HelperManifest) -> Result<(), LinuxError> {
    let abi = ABI::V3;
    let mut created = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map(|ruleset| ruleset.set_compatibility(CompatLevel::HardRequirement))
        .and_then(Ruleset::create)
        .map_err(|_| denied("Landlock ABI 3 ruleset creation failed"))?;
    for rule in manifest.landlock_rules() {
        let path = PathFd::new(rule.path())
            .map_err(|_| denied("Landlock could not open an exact rule path"))?;
        created = created
            .add_rule(PathBeneath::new(path, native_access(rule.access(), rule.path().is_dir())))
            .map_err(|_| denied("Landlock rejected a path-beneath rule"))?;
    }
    let status = created.restrict_self().map_err(|_| denied("Landlock restriction failed"))?;
    if status.ruleset != RulesetStatus::FullyEnforced || !status.no_new_privs {
        return Err(denied("Landlock or no-new-privileges was not fully enforced"));
    }
    Ok(())
}

fn native_access(access: LandlockAccess, directory: bool) -> landlock::BitFlags<AccessFs> {
    let bits = access.bits();
    let mut native = landlock::BitFlags::EMPTY;
    if bits & (1 << 0) != 0 {
        native |= AccessFs::Execute;
    }
    if bits & (1 << 1) != 0 {
        native |= AccessFs::ReadFile;
    }
    if directory && bits & (1 << 2) != 0 {
        native |= AccessFs::ReadDir;
    }
    if bits & (1 << 3) != 0 {
        native |= AccessFs::WriteFile;
    }
    if directory && bits & (1 << 4) != 0 {
        native |= AccessFs::MakeChar
            | AccessFs::MakeDir
            | AccessFs::MakeReg
            | AccessFs::MakeSock
            | AccessFs::MakeFifo
            | AccessFs::MakeBlock
            | AccessFs::MakeSym;
    }
    if bits & (1 << 5) != 0 {
        native |= if directory { AccessFs::RemoveDir } else { AccessFs::RemoveFile };
    }
    if directory && bits & (1 << 6) != 0 {
        native |= AccessFs::Refer;
    }
    if bits & (1 << 7) != 0 {
        native |= AccessFs::Truncate;
    }
    native
}

fn denied(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::SandboxDenied,
        LinuxOperation::Activate,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}
