//! Concrete cross-target release layouts and ownership contracts.

use crate::{
    Platform, QualificationError, QualificationErrorCode, QualificationRecovery, digest_bytes,
};

mod contracts;
mod path;

pub use contracts::{EntryKind, PathOwnership, PermissionContract};
pub use path::InstallPath;

/// One exact installed or protected path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutEntry {
    path: InstallPath,
    kind: EntryKind,
    ownership: PathOwnership,
    permissions: PermissionContract,
    preserve_on_uninstall: bool,
}

impl LayoutEntry {
    const fn new(
        path: InstallPath,
        kind: EntryKind,
        ownership: PathOwnership,
        permissions: PermissionContract,
        preserve_on_uninstall: bool,
    ) -> Self {
        Self { path, kind, ownership, permissions, preserve_on_uninstall }
    }

    /// Borrows the exact path.
    #[must_use]
    pub const fn path(&self) -> &InstallPath {
        &self.path
    }

    /// Returns the expected entry kind.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Returns the entry owner.
    #[must_use]
    pub const fn ownership(&self) -> PathOwnership {
        self.ownership
    }

    /// Returns the permission contract.
    #[must_use]
    pub const fn permissions(&self) -> PermissionContract {
        self.permissions
    }

    /// Reports whether ordinary uninstall must preserve the entry.
    #[must_use]
    pub const fn preserve_on_uninstall(&self) -> bool {
        self.preserve_on_uninstall
    }
}

/// Complete concrete per-user release layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseLayout {
    platform: Platform,
    binary_directory: InstallPath,
    helper_directory: InstallPath,
    config_file: InstallPath,
    state_root: InstallPath,
    log_root: InstallPath,
    service_definition: InstallPath,
    entries: Vec<LayoutEntry>,
}

impl ReleaseLayout {
    /// Constructs the reviewed per-user layout beneath an exact platform home directory.
    ///
    /// # Errors
    ///
    /// Rejects an invalid home path or any derived ownership overlap.
    pub fn production(platform: Platform, home: &InstallPath) -> Result<Self, QualificationError> {
        let ProductionPaths {
            binary_directory,
            helper_directory,
            config_file,
            state_root,
            log_root,
            service,
        } = production_paths(platform, home)?;
        let entry_paths = EntryPaths {
            binary_directory: &binary_directory,
            helper_directory: &helper_directory,
            config_file: &config_file,
            state_root: &state_root,
            log_root: &log_root,
            service: &service,
        };
        let mut entries =
            production_entries(platform, entry_paths, production_permissions(platform))?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        if entries.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(layout_error("release layout repeats a path"));
        }
        Ok(Self {
            platform,
            binary_directory,
            helper_directory,
            config_file,
            state_root,
            log_root,
            service_definition: service,
            entries,
        })
    }

    /// Returns the target platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Borrows the installed application binary directory.
    #[must_use]
    pub const fn binary_directory(&self) -> &InstallPath {
        &self.binary_directory
    }

    /// Borrows the native sandbox helper directory.
    #[must_use]
    pub const fn helper_directory(&self) -> &InstallPath {
        &self.helper_directory
    }

    /// Borrows the launcher-generated strict daemon configuration path.
    #[must_use]
    pub const fn config_file(&self) -> &InstallPath {
        &self.config_file
    }

    /// Borrows the runtime-owned protected state root.
    #[must_use]
    pub const fn state_root(&self) -> &InstallPath {
        &self.state_root
    }

    /// Borrows the runtime-owned protected log root.
    #[must_use]
    pub const fn log_root(&self) -> &InstallPath {
        &self.log_root
    }

    /// Borrows the installed optional supervisor-template path.
    #[must_use]
    pub const fn service_definition(&self) -> &InstallPath {
        &self.service_definition
    }

    /// Borrows all exact entries in path order.
    #[must_use]
    pub fn entries(&self) -> &[LayoutEntry] {
        &self.entries
    }

    /// Returns a deterministic digest of layout, ownership, kinds, and permissions.
    #[must_use]
    pub fn digest(&self) -> crate::Sha256Digest {
        let mut canonical =
            format!("peritus/release-layout/v1\nplatform={}\n", self.platform.as_str());
        for entry in &self.entries {
            use core::fmt::Write as _;
            writeln!(
                &mut canonical,
                "{}|{:?}|{:?}|{:?}|{}",
                entry.path.as_str(),
                entry.kind,
                entry.ownership,
                entry.permissions,
                entry.preserve_on_uninstall,
            )
            .expect("writing to String cannot fail");
        }
        digest_bytes(canonical.as_bytes()).sha256()
    }
}

struct ProductionPaths {
    binary_directory: InstallPath,
    helper_directory: InstallPath,
    config_file: InstallPath,
    state_root: InstallPath,
    log_root: InstallPath,
    service: InstallPath,
}

#[derive(Clone, Copy)]
struct EntryPaths<'path> {
    binary_directory: &'path InstallPath,
    helper_directory: &'path InstallPath,
    config_file: &'path InstallPath,
    state_root: &'path InstallPath,
    log_root: &'path InstallPath,
    service: &'path InstallPath,
}

#[derive(Clone, Copy)]
struct ProductionPermissions {
    executable: PermissionContract,
    private_directory: PermissionContract,
    private_file: PermissionContract,
}

fn production_paths(
    platform: Platform,
    home: &InstallPath,
) -> Result<ProductionPaths, QualificationError> {
    match platform {
        Platform::Linux => linux_paths(home),
        Platform::Macos => macos_paths(home),
        Platform::Windows => windows_paths(home),
    }
}

fn linux_paths(home: &InstallPath) -> Result<ProductionPaths, QualificationError> {
    Ok(ProductionPaths {
        binary_directory: home.join(Platform::Linux, ".local/bin")?,
        helper_directory: home.join(Platform::Linux, ".local/libexec/peritus")?,
        config_file: home.join(Platform::Linux, ".config/peritus/peritus.toml")?,
        state_root: home.join(Platform::Linux, ".local/state/peritus")?,
        log_root: home.join(Platform::Linux, ".local/state/peritus/log")?,
        service: home.join(Platform::Linux, ".local/share/peritus/peritus.service")?,
    })
}

fn macos_paths(home: &InstallPath) -> Result<ProductionPaths, QualificationError> {
    Ok(ProductionPaths {
        binary_directory: home.join(Platform::Macos, "Library/Application Support/Peritus/bin")?,
        helper_directory: home
            .join(Platform::Macos, "Library/Application Support/Peritus/libexec")?,
        config_file: home
            .join(Platform::Macos, "Library/Application Support/Peritus/config/peritus.toml")?,
        state_root: home.join(Platform::Macos, "Library/Application Support/Peritus/state")?,
        log_root: home.join(Platform::Macos, "Library/Logs/Peritus")?,
        service: home.join(
            Platform::Macos,
            "Library/Application Support/Peritus/share/peritus/com.corvidae.peritus.plist.in",
        )?,
    })
}

fn windows_paths(home: &InstallPath) -> Result<ProductionPaths, QualificationError> {
    Ok(ProductionPaths {
        binary_directory: home.join(Platform::Windows, "AppData/Local/Programs/Peritus/bin")?,
        helper_directory: home.join(Platform::Windows, "AppData/Local/Programs/Peritus/libexec")?,
        config_file: home.join(Platform::Windows, "AppData/Local/Peritus/config/peritus.toml")?,
        state_root: home.join(Platform::Windows, "AppData/Local/Peritus/state")?,
        log_root: home.join(Platform::Windows, "AppData/Local/Peritus/logs")?,
        service: home
            .join(Platform::Windows, "AppData/Local/Programs/Peritus/share/Peritus.Task.xml.in")?,
    })
}

const fn production_permissions(platform: Platform) -> ProductionPermissions {
    match platform {
        Platform::Linux | Platform::Macos => ProductionPermissions {
            executable: PermissionContract::UnixMode(0o755),
            private_directory: PermissionContract::UnixMode(0o700),
            private_file: PermissionContract::UnixMode(0o600),
        },
        Platform::Windows => ProductionPermissions {
            executable: PermissionContract::WindowsExecutable,
            private_directory: PermissionContract::WindowsOwnerOnly,
            private_file: PermissionContract::WindowsOwnerOnly,
        },
    }
}

fn production_entries(
    platform: Platform,
    paths: EntryPaths<'_>,
    permissions: ProductionPermissions,
) -> Result<Vec<LayoutEntry>, QualificationError> {
    let executable_suffix = if platform == Platform::Windows { ".exe" } else { "" };
    let helper_name = match platform {
        Platform::Linux => "peritus-linux-sandbox-helper",
        Platform::Macos => "peritus-macos-sandbox-helper",
        Platform::Windows => "peritus-windows-sandbox-helper.exe",
    };
    Ok(vec![
        LayoutEntry::new(
            paths.binary_directory.clone(),
            EntryKind::Directory,
            PathOwnership::Package,
            permissions.private_directory,
            false,
        ),
        LayoutEntry::new(
            paths.helper_directory.clone(),
            EntryKind::Directory,
            PathOwnership::Package,
            permissions.private_directory,
            false,
        ),
        LayoutEntry::new(
            paths.binary_directory.join(platform, &format!("peritusd{executable_suffix}"))?,
            EntryKind::Executable,
            PathOwnership::Package,
            permissions.executable,
            false,
        ),
        LayoutEntry::new(
            paths.binary_directory.join(platform, &format!("peritus{executable_suffix}"))?,
            EntryKind::Executable,
            PathOwnership::Package,
            permissions.executable,
            false,
        ),
        LayoutEntry::new(
            paths.binary_directory.join(platform, &format!("peritus-tui{executable_suffix}"))?,
            EntryKind::Executable,
            PathOwnership::Package,
            permissions.executable,
            false,
        ),
        LayoutEntry::new(
            paths.helper_directory.join(platform, helper_name)?,
            EntryKind::Executable,
            PathOwnership::Package,
            permissions.executable,
            false,
        ),
        LayoutEntry::new(
            paths.config_file.clone(),
            EntryKind::File,
            PathOwnership::Runtime,
            permissions.private_file,
            true,
        ),
        LayoutEntry::new(
            paths.state_root.clone(),
            EntryKind::Directory,
            PathOwnership::Runtime,
            permissions.private_directory,
            true,
        ),
        LayoutEntry::new(
            paths.log_root.clone(),
            EntryKind::Directory,
            PathOwnership::Runtime,
            permissions.private_directory,
            true,
        ),
        LayoutEntry::new(
            paths.service.clone(),
            EntryKind::File,
            PathOwnership::Package,
            permissions.private_file,
            false,
        ),
    ])
}

fn layout_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Layout,
        QualificationRecovery::CorrectInput,
        "validate release layout",
        detail,
    )
}
