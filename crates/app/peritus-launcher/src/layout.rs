//! Platform-native application directory discovery.

use std::{env, fs, path::PathBuf};

use crate::LauncherError;

/// Protected platform-local roots used by the Peritus product composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppLayout {
    configuration: PathBuf,
    state: PathBuf,
    cache: PathBuf,
}

impl AppLayout {
    /// Discovers native application roots without requiring user-provided exports or config.
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError::PlatformPaths`] if the current platform has no usable absolute
    /// per-user application directory.
    pub fn discover() -> Result<Self, LauncherError> {
        platform_layout()
    }

    /// Creates, protects, and canonicalizes all application roots.
    ///
    /// # Errors
    ///
    /// Returns an exact filesystem failure when a root cannot be prepared.
    pub fn prepare(self) -> Result<Self, LauncherError> {
        let configuration = prepare_root(self.configuration)?;
        let state = prepare_root(self.state)?;
        let cache = prepare_root(self.cache)?;
        let prepared = Self { configuration, state, cache };
        prepare_root(prepared.logs_root())?;
        Ok(prepared)
    }

    /// Borrows the protected configuration root.
    #[must_use]
    pub fn config_root(&self) -> &std::path::Path {
        &self.configuration
    }

    /// Borrows the protected durable state root.
    #[must_use]
    pub fn state_root(&self) -> &std::path::Path {
        &self.state
    }

    /// Borrows the protected disposable cache root.
    #[must_use]
    pub fn cache_root(&self) -> &std::path::Path {
        &self.cache
    }

    /// Returns the immutable-generation product-state directory.
    #[must_use]
    pub fn product_state_root(&self) -> PathBuf {
        self.state.join("product-state")
    }

    /// Returns one immutable-generation strict daemon configuration path.
    #[must_use]
    pub fn daemon_config(&self, generation: u64) -> PathBuf {
        self.configuration.join(format!("peritus-{generation:020}.toml"))
    }

    /// Returns the generated public approval-registry path.
    #[must_use]
    pub fn approval_registry(&self) -> PathBuf {
        self.state.join("approval-registry.bin")
    }

    /// Returns the retained application log directory.
    #[must_use]
    pub fn logs_root(&self) -> PathBuf {
        self.state.join("logs")
    }

    /// Returns the retained daemon diagnostic log path.
    #[must_use]
    pub fn daemon_log(&self) -> PathBuf {
        self.logs_root().join("peritusd.log")
    }

    #[cfg(test)]
    pub(crate) fn for_test(root: &std::path::Path) -> Self {
        Self {
            configuration: root.join("config"),
            state: root.join("state"),
            cache: root.join("cache"),
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_layout() -> Result<AppLayout, LauncherError> {
    let home = absolute_environment("HOME")?;
    let support = home.join("Library/Application Support/Peritus");
    Ok(AppLayout {
        configuration: support.join("Config"),
        state: support.join("State"),
        cache: home.join("Library/Caches/Peritus"),
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_layout() -> Result<AppLayout, LauncherError> {
    let home = absolute_environment("HOME")?;
    let config =
        optional_absolute_environment("XDG_CONFIG_HOME")?.unwrap_or_else(|| home.join(".config"));
    let state = optional_absolute_environment("XDG_STATE_HOME")?
        .unwrap_or_else(|| home.join(".local/state"));
    let cache =
        optional_absolute_environment("XDG_CACHE_HOME")?.unwrap_or_else(|| home.join(".cache"));
    Ok(AppLayout {
        configuration: config.join("peritus"),
        state: state.join("peritus"),
        cache: cache.join("peritus"),
    })
}

#[cfg(windows)]
fn platform_layout() -> Result<AppLayout, LauncherError> {
    let roaming = absolute_environment("APPDATA")?;
    let local = absolute_environment("LOCALAPPDATA")?;
    Ok(AppLayout {
        configuration: roaming.join("Peritus"),
        state: local.join("Peritus/State"),
        cache: local.join("Peritus/Cache"),
    })
}

fn absolute_environment(name: &'static str) -> Result<PathBuf, LauncherError> {
    optional_absolute_environment(name)?.ok_or_else(|| {
        LauncherError::PlatformPaths(format!("{name} is unavailable for the current user"))
    })
}

fn optional_absolute_environment(name: &'static str) -> Result<Option<PathBuf>, LauncherError> {
    let Some(value) = env::var_os(name) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(LauncherError::PlatformPaths(format!("{name} must contain an absolute path")));
    }
    Ok(Some(path))
}

fn prepare_root(path: PathBuf) -> Result<PathBuf, LauncherError> {
    fs::create_dir_all(&path)
        .map_err(|error| LauncherError::filesystem("create application directory", &path, error))?;
    protect_directory(&path)?;
    fs::canonicalize(&path).map_err(|error| {
        LauncherError::filesystem("canonicalize application directory", path, error)
    })
}

#[cfg(unix)]
fn protect_directory(path: &std::path::Path) -> Result<(), LauncherError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| LauncherError::filesystem("protect application directory", path, error))
}

#[cfg(windows)]
const fn protect_directory(_path: &std::path::Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_creates_separate_protected_roots() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("prepare layout");
        assert!(layout.config_root().is_dir());
        assert!(layout.state_root().is_dir());
        assert!(layout.cache_root().is_dir());
        assert!(layout.logs_root().is_dir());
        assert_ne!(layout.config_root(), layout.state_root());
    }
}
