use std::{
    fmt::Write as _,
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::release::{TemporaryDirectory, digest};

pub(super) struct Fixture {
    _temporary: TemporaryDirectory,
    pub(super) home: PathBuf,
    pub(super) commands: PathBuf,
    pub(super) bundle: PathBuf,
    pub(super) archive: PathBuf,
    pub(super) checksum: PathBuf,
    pub(super) log: PathBuf,
    bootstrap: PathBuf,
    release_base: String,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temporary = TemporaryDirectory::new("peritus-installer-test").expect("fixture");
        let root = temporary.path();
        let home = root.join("user home");
        let commands = root.join("commands");
        let platform = if cfg!(target_os = "macos") { "macos" } else { "linux" };
        let name = format!("peritus-{platform}-{}", std::env::consts::ARCH);
        let bundle = root.join("packages").join(&name);
        let release_root = root.join("releases/v1.2.3");
        let archive = release_root.join(format!("{name}.tar.gz"));
        let checksum = release_root.join(format!("{name}.tar.gz.sha256"));
        for directory in [&home, &commands, &bundle, &release_root] {
            fs::create_dir_all(directory).expect("fixture directory");
        }
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
        let source = fs::read_to_string(workspace.join("install.sh")).expect("bootstrap");
        let bootstrap = root.join("install.sh");
        fs::write(&bootstrap, source.replace("@PERITUS_RELEASE_TAG@", "v1.2.3")).expect("bind tag");
        let log = root.join("commands.log");
        let release_base = format!("file://{}", root.join("releases").display());
        let fixture = Self {
            home,
            commands,
            bundle,
            archive,
            checksum,
            log,
            bootstrap,
            release_base,
            _temporary: temporary,
        };
        fixture.prepare_commands();
        fixture.prepare_bundle(workspace, platform);
        fixture.repack();
        fixture
    }

    fn prepare_commands(&self) {
        for name in [
            "tar", "sed", "tr", "mktemp", "rm", "sh", "uname", "cp", "mv", "ln", "install",
            "mkdir", "grep", "dirname", "sleep", "gzip", "rmdir",
        ] {
            symlink(system_command(name), self.commands.join(name)).expect("fixture command");
        }
        let hash_command = if cfg!(target_os = "macos") { "shasum" } else { "sha256sum" };
        symlink(system_command(hash_command), self.commands.join(hash_command)).expect("hash tool");
        self.mock("id", "printf '1000\\n'\n");
        for name in ["git", "bwrap", "dbus-daemon", "kwalletd6"] {
            self.mock(name, "printf 'fixture version\\n'\n");
        }
        self.mock("sudo", "printf '%s\\n' \"$*\" >> \"$PERITUS_TEST_LOG\"\n[ \"$1\" != -- ] || shift\nexec \"$@\"\n");
        let curl = system_command("curl");
        self.mock("curl", &format!(
            "for argument do case \"$argument\" in https:*|http:*) exit 99 ;; esac; done\nexec '{}' \"$@\"\n",
            curl.display()
        ));
    }

    fn prepare_bundle(&self, workspace: &Path, platform: &str) {
        let mut paths = Vec::new();
        for name in ["Install-Peritus.sh", "Upgrade-Peritus.sh", "Uninstall-Peritus.sh"] {
            let source = workspace.join("packaging").join(platform).join(name);
            let target = self.bundle.join(name);
            fs::copy(source, &target).expect("lifecycle adapter");
            executable(&target);
            paths.push(name.to_owned());
        }
        for name in ["peritus", "peritusd", "peritus-tui"] {
            paths.push(format!("bin/{name}"));
        }
        paths.push(format!("libexec/peritus-{platform}-sandbox-helper"));
        let service =
            if platform == "macos" { "com.corvidae.peritus.plist.in" } else { "peritus.service" };
        paths.push(format!("share/peritus/{service}"));
        paths.push("manifest.toml".to_owned());
        for path in &paths[3..] {
            let target = self.bundle.join(path);
            fs::create_dir_all(target.parent().expect("parent")).expect("artifact directory");
            fs::write(&target, "#!/bin/sh\nprintf 'peritus 1.2.3\\n'\n").expect("fixture artifact");
            executable(&target);
        }
        let mut sums = String::new();
        for path in &paths {
            writeln!(sums, "{}  {path}", digest(&self.bundle.join(path)).expect("artifact digest"))
                .expect("format checksum");
        }
        fs::write(self.bundle.join("SHA256SUMS"), sums).expect("package checksums");
    }

    pub(super) fn mock(&self, name: &str, body: &str) {
        let path = self.commands.join(name);
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}")).expect("mock command");
        executable(&path);
    }

    pub(super) fn repack(&self) {
        let status = Command::new(system_command("tar"))
            .arg("-C")
            .arg(self.bundle.parent().expect("package parent"))
            .arg("-czf")
            .arg(&self.archive)
            .arg(self.bundle.file_name().expect("package name"))
            .status()
            .expect("archive fixture");
        assert!(status.success());
        fs::write(&self.checksum, format!("{}\n", digest(&self.archive).expect("archive digest")))
            .expect("archive checksum");
    }

    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(system_command("sh"));
        command
            .arg(&self.bootstrap)
            .env("HOME", &self.home)
            .env("PATH", &self.commands)
            .env("SHELL", "/bin/sh")
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("PERITUS_RELEASE_BASE_URL", &self.release_base)
            .env("PERITUS_TEST_BIN", &self.commands)
            .env("PERITUS_TEST_LOG", &self.log)
            .env("PERITUS_INSTALL_DEPS", "1")
            .env_remove("PERITUS_VERSION");
        command
    }

    pub(super) fn run(&self) -> Output {
        self.command().output().expect("run bootstrap fixture")
    }

    pub(super) fn installed(&self) -> PathBuf {
        self.home.join(".local/bin/peritus")
    }

    pub(super) fn uninstall(&self) -> Output {
        let share = if cfg!(target_os = "macos") {
            self.home.join("Library/Application Support/Peritus/share/peritus")
        } else {
            self.home.join(".local/share/peritus")
        };
        let mut command = Command::new(system_command("sh"));
        command
            .arg(share.join("Uninstall-Peritus.sh"))
            .env("HOME", &self.home)
            .env("PATH", &self.commands);
        command.output().expect("run retained uninstaller")
    }
}

fn executable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("fixture executable");
}

fn system_command(name: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", "command -v -- \"$1\"", "fixture", name])
        .output()
        .expect("find fixture utility");
    assert!(output.status.success(), "missing fixture utility {name}");
    PathBuf::from(String::from_utf8(output.stdout).expect("command path").trim())
}

pub(super) fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
