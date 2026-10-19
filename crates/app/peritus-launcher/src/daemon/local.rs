//! Read-only discovery and orderly withdrawal of this user's packaged daemon.

use std::{fs, path::PathBuf, time::Duration};

use crate::{AppLayout, LauncherError};

impl AppLayout {
    /// Serializes a local stop with other stops and packaged daemon launches.
    ///
    /// Keep the returned handle alive until the operation completes. A missing daemon
    /// directory returns `None` without initializing an installation.
    ///
    /// # Errors
    /// Returns filesystem errors or a timeout acquiring lifecycle ownership.
    pub async fn lock_daemon_lifecycle(
        &self,
        timeout: Duration,
    ) -> Result<Option<fs::File>, LauncherError> {
        let root = self.state_root().join("daemon");
        match fs::metadata(&root) {
            Ok(_) => super::catalog::lifecycle_lock(&root, timeout).await.map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(LauncherError::filesystem("inspect local daemon directory", root, error))
            }
        }
    }

    /// Finds the running packaged daemon without setup, provider discovery, or a new launch.
    ///
    /// # Errors
    /// Returns an exact filesystem error or rejects an invalid live-instance descriptor.
    pub fn running_daemon_endpoint(&self) -> Result<Option<PathBuf>, LauncherError> {
        let root = self.state_root().join("daemon");
        if super::instance_lock_available(&root)? {
            return Ok(None);
        }
        let path = root.join("daemon.instance");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(LauncherError::filesystem("read live daemon descriptor", path, error));
            }
        };
        let mut lines = text.lines();
        let name = lines
            .next()
            .filter(|line| *line == "version=1")
            .and_then(|_| lines.next())
            .and_then(|line| line.strip_prefix("endpoint="))
            .filter(|name| {
                name.strip_prefix("peritus-").is_some_and(|suffix| {
                    suffix.len() == 32
                        && suffix
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                })
            })
            .ok_or_else(|| {
                LauncherError::PlatformPaths(format!(
                    "invalid live daemon endpoint descriptor: {}",
                    path.display()
                ))
            })?;
        #[cfg(unix)]
        let endpoint = {
            // Startup canonicalizes the application state root before deriving a shortened
            // address. Use those same bytes when HOME/XDG paths contain symlinks.
            let state = fs::canonicalize(self.state_root()).map_err(|error| {
                LauncherError::filesystem("resolve local daemon state", self.state_root(), error)
            })?;
            let original = state.join("daemon").join(format!("{name}.sock"));
            peritus_local_socket::bounded_path(
                &original,
                peritus_local_socket::NATIVE_MAX_PATH_BYTES,
            )
            .map_err(|error| {
                LauncherError::filesystem("derive local daemon endpoint", original, error)
            })?
        };
        #[cfg(windows)]
        let endpoint = PathBuf::from(format!(r"\\.\pipe\{name}"));
        Ok(Some(endpoint))
    }

    /// Waits until the daemon releases ownership of this user's local state.
    ///
    /// # Errors
    /// Returns filesystem errors or a timeout instead of claiming an unfinished stop succeeded.
    pub async fn wait_for_daemon_shutdown(&self, timeout: Duration) -> Result<(), LauncherError> {
        let root = self.state_root().join("daemon");
        let started = tokio::time::Instant::now();
        while !super::instance_lock_available(&root)? {
            if started.elapsed() >= timeout {
                return Err(LauncherError::DaemonSpawn(format!(
                    "daemon shutdown has not completed; diagnostics: {}",
                    self.daemon_log().display()
                )));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(())
    }
}
