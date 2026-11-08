//! Host-owned lifecycle for explicitly allowlisted external module applications.
//!
//! Module code selects only an application identity and an operation. The host owns the pinned
//! release, license identity, download verification, installation paths, prerequisite checks,
//! process authority, and lifecycle observations.

use super::ProductRunInput;
use crate::control::PermissionCapability;
use peritus_modules::{Registry, preparation};
use peritus_plugin_sdk::ModuleHostOutcome;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write as _,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    os::unix::fs::PermissionsExt as _,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const APPLICATION_ID: &str = "artex";
const MODULE_ID: &str = "peritus.red-team";
const VERSION: &str = "0.5.1";
const LICENSE_EXPRESSION: &str = "AGPL-3.0-only";
const LICENSE_SHA256: &str = "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0";
const LICENSE_URL: &str = "https://github.com/BaaRai/ARTEX/blob/v0.5.1/LICENSE";
const RELEASE_URL_AMD64: &str =
    "https://github.com/BaaRai/ARTEX/releases/download/v0.5.1/artex-0.5.1-linux-amd64.zip";
const RELEASE_SHA256_AMD64: &str =
    "78a412f0f851e1eb2f2c099205e95c473ef99d00bc6077cae787666942eb2758";
const RELEASE_URL_ARM64: &str =
    "https://github.com/BaaRai/ARTEX/releases/download/v0.5.1/artex-0.5.1-linux-arm64.zip";
const RELEASE_SHA256_ARM64: &str =
    "256d93d9e5df02b47d0eb23617985b292851286907e8b2abfea22d8d2e60eb59";
const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024;
const APP_PORT: u16 = 8787;
const PROXY_PORT: u16 = 8788;
const FIRST_POSTGRES_PORT: u16 = 55_432;
const LAST_POSTGRES_PORT: u16 = 55_532;

#[derive(Debug)]
enum Failure {
    Rejected(String),
    Failed(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LicenseReceipt {
    schema: u16,
    application: String,
    version: String,
    expression: String,
    license_url: String,
    license_sha256: String,
    accepted_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    schema: u16,
    application: String,
    version: String,
    archive_sha256: String,
    architecture: String,
    application_port: u16,
    proxy_port: u16,
    postgres_port: u16,
    installed_at_unix: u64,
}

pub(super) fn dispatch(
    input: &ProductRunInput,
    module_id: &str,
    operation: &str,
    request: &Value,
) -> ModuleHostOutcome {
    match Broker::new(input, module_id).and_then(|broker| broker.dispatch(operation, request)) {
        Ok(output) => ModuleHostOutcome::Success { output },
        Err(Failure::Rejected(detail)) => {
            ModuleHostOutcome::Rejected { code: "external_application_rejected".into(), detail }
        }
        Err(Failure::Failed(detail)) => ModuleHostOutcome::Failed {
            code: "external_application_failed".into(),
            detail,
            retryable_with_new_action: true,
        },
    }
}

struct Broker<'a> {
    input: &'a ProductRunInput,
    root: PathBuf,
}

impl<'a> Broker<'a> {
    fn new(input: &'a ProductRunInput, module_id: &str) -> Result<Self, Failure> {
        if module_id != MODULE_ID {
            return Err(Failure::Rejected(
                "this external application is available only to the Red Team module".into(),
            ));
        }
        let root = Registry::for_user()
            .map_err(|error| Failure::Failed(error.to_string()))?
            .root()
            .join("integrations")
            .join(APPLICATION_ID);
        Ok(Self { input, root })
    }

    fn dispatch(&self, operation: &str, request: &Value) -> Result<Value, Failure> {
        if request.get("application").and_then(Value::as_str) != Some(APPLICATION_ID) {
            return Err(Failure::Rejected("unknown external application identity".into()));
        }
        match operation {
            "inspect" => {
                self.require(&[PermissionCapability::Read])?;
                self.inspect()
            }
            "accept_license" => {
                self.require(&[PermissionCapability::Read, PermissionCapability::Write])?;
                self.accept_license(request)
            }
            "install" => {
                self.require(&[
                    PermissionCapability::Read,
                    PermissionCapability::Write,
                    PermissionCapability::Process,
                    PermissionCapability::Network,
                ])?;
                self.install()?;
                self.inspect()
            }
            "start" => {
                self.require(&[
                    PermissionCapability::Read,
                    PermissionCapability::Write,
                    PermissionCapability::Process,
                ])?;
                self.start()?;
                self.inspect()
            }
            "stop" => {
                self.require(&[PermissionCapability::Read, PermissionCapability::Process])?;
                self.stop()?;
                self.inspect()
            }
            "logs" => {
                self.require(&[PermissionCapability::Read])?;
                self.logs()
            }
            _ => Err(Failure::Rejected("unsupported external application operation".into())),
        }
    }

    fn require(&self, capabilities: &[PermissionCapability]) -> Result<(), Failure> {
        let permissions = self.input.conversation.effective_permissions();
        if let Some(capability) =
            capabilities.iter().find(|capability| !permissions.allows(**capability))
        {
            return Err(Failure::Rejected(format!(
                "ARTEX operation requires current {} permission",
                match capability {
                    PermissionCapability::Read => "read",
                    PermissionCapability::Write => "write",
                    PermissionCapability::Process => "process",
                    PermissionCapability::Network => "network",
                }
            )));
        }
        Ok(())
    }

    fn inspect(&self) -> Result<Value, Failure> {
        let accepted = self.license_receipt()?.is_some();
        let installation = self.installation()?;
        let prerequisites = prerequisites();
        let installed = installation.is_some();
        let supervisor_running = self.supervisor_running()?;
        let database_running = installation
            .as_ref()
            .is_some_and(|_| postgres_running(&self.current().join("postgres")));
        let healthy = installed && health(APP_PORT);
        Ok(json!({
            "schema":1,
            "application":APPLICATION_ID,
            "name":"ARTEX",
            "version":VERSION,
            "license":{
                "expression":LICENSE_EXPRESSION,
                "sha256":LICENSE_SHA256,
                "url":LICENSE_URL,
                "accepted":accepted,
            },
            "installed":installed,
            "installation":installation,
            "running":supervisor_running,
            "unexpected_healthy_endpoint":healthy && !supervisor_running,
            "database_running":database_running,
            "healthy":healthy,
            "url":format!("http://127.0.0.1:{APP_PORT}"),
            "prerequisites":prerequisites,
            "logs_available":self.current().join("artex.log").is_file(),
        }))
    }

    fn accept_license(&self, request: &Value) -> Result<Value, Failure> {
        let digest = request.get("license_sha256").and_then(Value::as_str).unwrap_or_default();
        let version = request.get("version").and_then(Value::as_str).unwrap_or_default();
        if digest != LICENSE_SHA256 || version != VERSION {
            return Err(Failure::Rejected(
                "license acceptance does not match the pinned ARTEX release and license bytes"
                    .into(),
            ));
        }
        fs::create_dir_all(&self.root).map_err(io_failed)?;
        private_directory(&self.root)?;
        let receipt = LicenseReceipt {
            schema: 1,
            application: APPLICATION_ID.into(),
            version: VERSION.into(),
            expression: LICENSE_EXPRESSION.into(),
            license_url: LICENSE_URL.into(),
            license_sha256: LICENSE_SHA256.into(),
            accepted_at_unix: now()?,
        };
        publish_json(&self.root.join("license-acceptance.json"), &receipt)?;
        self.inspect()
    }

    fn install(&self) -> Result<(), Failure> {
        if self.license_receipt()?.is_none() {
            return Err(Failure::Rejected(
                "accept the exact ARTEX license in Peritus before installation".into(),
            ));
        }
        if let Some(installed) = self.installation()? {
            if installed.version == VERSION && installed.archive_sha256 == release()?.1 {
                return Ok(());
            }
            return Err(Failure::Rejected(
                "a different ARTEX installation is retained; stop and remove it explicitly before replacing it"
                    .into(),
            ));
        }
        let missing = prerequisites()
            .into_iter()
            .filter_map(|value| {
                (!value.get("available").and_then(Value::as_bool).unwrap_or(false)).then(|| {
                    value.get("name").and_then(Value::as_str).unwrap_or("unknown").to_owned()
                })
            })
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(Failure::Rejected(format!(
                "ARTEX native installation is missing required local programs: {}",
                missing.join(", ")
            )));
        }
        if !port_available(APP_PORT) || !port_available(PROXY_PORT) {
            return Err(Failure::Rejected(
                "ARTEX needs loopback ports 8787 and 8788, but at least one is already occupied"
                    .into(),
            ));
        }
        fs::create_dir_all(&self.root).map_err(io_failed)?;
        private_directory(&self.root)?;
        let temporary = tempfile::Builder::new()
            .prefix("install-")
            .tempdir_in(&self.root)
            .map_err(io_failed)?;
        let staging = temporary.path();
        let (url, digest, architecture) = release()?;
        let fetched = preparation::fetch(
            &preparation::Specification {
                url: url.into(),
                expected_sha256: Some(digest.into()),
                maximum_bytes: MAX_ARCHIVE_BYTES,
            },
            &Arc::new(AtomicBool::new(false)),
        )
        .map_err(preparation_failed)?;
        let archive = staging.join("artex.zip");
        publish_bytes(&archive, &fetched.bytes)?;
        let extracted = staging.join("extracted");
        extract_archive(&archive, &extracted)?;
        let application = extracted_root(&extracted)?;
        let installed_application = staging.join("app");
        fs::rename(&application, &installed_application).map_err(io_failed)?;
        let binary = installed_application.join("artex");
        if !binary.is_file() {
            return Err(Failure::Rejected("verified ARTEX archive has no artex executable".into()));
        }
        let smoke = Command::new(&binary)
            .arg("-h")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(command_failed("execute the verified ARTEX release"))?;
        if !smoke.success() {
            return Err(Failure::Rejected(
                "the verified ARTEX release binary failed its help smoke test".into(),
            ));
        }
        let postgres_port = available_postgres_port()?;
        initialize_postgres(staging, &self.root.join("socket"), postgres_port)?;
        write_config(staging, &self.current(), postgres_port)?;
        fs::create_dir_all(staging.join("data")).map_err(io_failed)?;
        fs::create_dir_all(staging.join("keys")).map_err(io_failed)?;
        let installation = Installation {
            schema: 1,
            application: APPLICATION_ID.into(),
            version: VERSION.into(),
            archive_sha256: digest.into(),
            architecture: architecture.into(),
            application_port: APP_PORT,
            proxy_port: PROXY_PORT,
            postgres_port,
            installed_at_unix: now()?,
        };
        publish_json(&staging.join("installation.json"), &installation)?;
        fs::remove_file(&archive).map_err(io_failed)?;
        let current = self.current();
        if current.exists() {
            return Err(Failure::Rejected("ARTEX installation appeared concurrently".into()));
        }
        fs::rename(staging, &current).map_err(io_failed)?;
        Ok(())
    }

    fn start(&self) -> Result<(), Failure> {
        let installation = self
            .installation()?
            .ok_or_else(|| Failure::Rejected("install ARTEX before starting it".into()))?;
        if self.supervisor_running()? {
            return Ok(());
        }
        if health(installation.application_port) {
            return Err(Failure::Rejected(
                "ARTEX health endpoint answers, but Peritus does not own its process; it was not reused"
                    .into(),
            ));
        }
        if !port_available(installation.application_port)
            || !port_available(installation.proxy_port)
        {
            return Err(Failure::Rejected(
                "ARTEX loopback ports are occupied by another process".into(),
            ));
        }
        start_postgres(&self.current().join("postgres"))?;
        let application = self.current().join("app");
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.current().join("artex.log"))
            .map_err(io_failed)?;
        let stderr = log.try_clone().map_err(io_failed)?;
        let mut command = Command::new(executable("setsid")?);
        command
            .arg(application.join("start.sh"))
            .arg("-addr")
            .arg(format!("127.0.0.1:{}", installation.application_port))
            .arg("-proxy")
            .arg(format!("127.0.0.1:{}", installation.proxy_port))
            .arg("-data")
            .arg(self.current().join("data"))
            .current_dir(&application)
            .env_clear()
            .env("ARTEX_CONFIG", application.join("config.json"))
            .env("ARTEX_JWT_KEY_DIR", self.current().join("keys"))
            .env("HOME", env::var_os("HOME").unwrap_or_default())
            .env("PATH", env::var_os("PATH").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = stop_postgres(&self.current().join("postgres"));
                return Err(command_failed("start ARTEX")(error));
            }
        };
        publish_bytes(&self.current().join("supervisor.pid"), child.id().to_string().as_bytes())?;
        for _ in 0..100 {
            if health(installation.application_port) {
                return Ok(());
            }
            if !process_owned(child.id(), &application) {
                self.cleanup_failed_start();
                return Err(Failure::Failed(
                    "ARTEX exited during startup; inspect the retained ARTEX log".into(),
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
        self.cleanup_failed_start();
        Err(Failure::Failed(
            "ARTEX did not become healthy within ten seconds; inspect its retained log".into(),
        ))
    }

    fn stop(&self) -> Result<(), Failure> {
        if let Some(pid) = self.supervisor_pid()? {
            let application = self.current().join("app");
            if process_owned(pid, &application) {
                let status = Command::new(executable("kill")?)
                    .arg("-TERM")
                    .arg(format!("-{pid}"))
                    .status()
                    .map_err(command_failed("stop ARTEX"))?;
                if !status.success() {
                    return Err(Failure::Failed("ARTEX supervisor rejected SIGTERM".into()));
                }
                for _ in 0..100 {
                    if !process_owned(pid, &application) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if process_owned(pid, &application) {
                    return Err(Failure::Failed(
                        "ARTEX did not stop within ten seconds; no forced kill was issued".into(),
                    ));
                }
            }
            let _ = fs::remove_file(self.current().join("supervisor.pid"));
            if health(APP_PORT) {
                return Err(Failure::Rejected(
                    "port 8787 still answers but Peritus does not own that process; it was not stopped"
                        .into(),
                ));
            }
        } else if health(APP_PORT) {
            return Err(Failure::Rejected(
                "port 8787 answers but Peritus does not own that ARTEX process; it was not stopped"
                    .into(),
            ));
        }
        stop_postgres(&self.current().join("postgres"))
    }

    fn logs(&self) -> Result<Value, Failure> {
        let path = self.current().join("artex.log");
        let mut bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(io_failed(error)),
        };
        if bytes.len() > 64 * 1024 {
            bytes.drain(..bytes.len() - 64 * 1024);
        }
        Ok(json!({
            "schema":1,
            "application":APPLICATION_ID,
            "logs":String::from_utf8_lossy(&bytes),
            "truncated":fs::metadata(path).is_ok_and(|metadata| metadata.len() > 64 * 1024),
        }))
    }

    fn current(&self) -> PathBuf {
        self.root.join("current")
    }

    fn license_receipt(&self) -> Result<Option<LicenseReceipt>, Failure> {
        read_json::<LicenseReceipt>(&self.root.join("license-acceptance.json")).and_then(
            |receipt| {
                if let Some(receipt) = &receipt
                    && (receipt.schema != 1
                        || receipt.application != APPLICATION_ID
                        || receipt.version != VERSION
                        || receipt.expression != LICENSE_EXPRESSION
                        || receipt.license_url != LICENSE_URL
                        || receipt.license_sha256 != LICENSE_SHA256)
                {
                    return Err(Failure::Rejected(
                        "stored ARTEX license receipt is stale or invalid".into(),
                    ));
                }
                Ok(receipt)
            },
        )
    }

    fn installation(&self) -> Result<Option<Installation>, Failure> {
        let current = self.current();
        if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(Failure::Rejected(
                "stored ARTEX installation root must not be a symbolic link".into(),
            ));
        }
        let installation = read_json::<Installation>(&current.join("installation.json"))?;
        if let Some(installation) = &installation {
            let (_, digest, architecture) = release()?;
            if installation.schema != 1
                || installation.application != APPLICATION_ID
                || installation.version != VERSION
                || installation.archive_sha256 != digest
                || installation.architecture != architecture
                || installation.application_port != APP_PORT
                || installation.proxy_port != PROXY_PORT
                || !(FIRST_POSTGRES_PORT..=LAST_POSTGRES_PORT).contains(&installation.postgres_port)
            {
                return Err(Failure::Rejected(
                    "stored ARTEX installation identity or configuration is invalid".into(),
                ));
            }
        }
        Ok(installation)
    }

    fn supervisor_pid(&self) -> Result<Option<u32>, Failure> {
        match fs::read_to_string(self.current().join("supervisor.pid")) {
            Ok(value) => value.trim().parse::<u32>().map(Some).map_err(|_| {
                Failure::Rejected("stored ARTEX supervisor identity is invalid".into())
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_failed(error)),
        }
    }

    fn supervisor_running(&self) -> Result<bool, Failure> {
        Ok(self
            .supervisor_pid()?
            .is_some_and(|pid| process_owned(pid, &self.current().join("app"))))
    }

    fn cleanup_failed_start(&self) {
        let _ = fs::remove_file(self.current().join("supervisor.pid"));
        let _ = stop_postgres(&self.current().join("postgres"));
    }
}

fn release() -> Result<(&'static str, &'static str, &'static str), Failure> {
    match env::consts::ARCH {
        "x86_64" => Ok((RELEASE_URL_AMD64, RELEASE_SHA256_AMD64, "amd64")),
        "aarch64" => Ok((RELEASE_URL_ARM64, RELEASE_SHA256_ARM64, "arm64")),
        other => Err(Failure::Rejected(format!(
            "ARTEX v{VERSION} has no pinned Linux release for architecture {other}"
        ))),
    }
}

fn prerequisites() -> Vec<Value> {
    ["unzip", "initdb", "pg_ctl", "createdb", "setsid", "kill"]
        .into_iter()
        .map(|name| json!({"name":name,"available":executable(name).is_ok()}))
        .collect()
}

fn executable(name: &str) -> Result<PathBuf, Failure> {
    let paths = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&paths)
        .map(|path| path.join(name))
        .find(|path| {
            path.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .ok_or_else(|| Failure::Rejected(format!("required local program `{name}` is unavailable")))
}

fn extract_archive(archive: &Path, destination: &Path) -> Result<(), Failure> {
    let unzip = executable("unzip")?;
    let listing = Command::new(&unzip)
        .arg("-Z1")
        .arg(archive)
        .output()
        .map_err(command_failed("inspect the verified ARTEX archive"))?;
    if !listing.status.success() || listing.stdout.len() > 1024 * 1024 {
        return Err(Failure::Rejected("ARTEX archive inventory is invalid or oversized".into()));
    }
    let listing = String::from_utf8(listing.stdout)
        .map_err(|_| Failure::Rejected("ARTEX archive inventory is not UTF-8".into()))?;
    let entries = listing.lines().collect::<Vec<_>>();
    if entries.is_empty() || entries.len() > 10_000 {
        return Err(Failure::Rejected("ARTEX archive entry count is invalid".into()));
    }
    for entry in entries {
        let path = Path::new(entry);
        if entry.contains('\\')
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(Failure::Rejected("ARTEX archive contains an unsafe path".into()));
        }
    }
    fs::create_dir_all(destination).map_err(io_failed)?;
    let status = Command::new(unzip)
        .args(["-q", "-n"])
        .arg(archive)
        .arg("-d")
        .arg(destination)
        .status()
        .map_err(command_failed("extract the verified ARTEX archive"))?;
    if !status.success() {
        return Err(Failure::Failed("verified ARTEX archive extraction failed".into()));
    }
    Ok(())
}

fn extracted_root(extracted: &Path) -> Result<PathBuf, Failure> {
    let mut entries = fs::read_dir(extracted)
        .map_err(io_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_failed)?;
    if entries.len() != 1 || !entries[0].file_type().map_err(io_failed)?.is_dir() {
        return Err(Failure::Rejected("ARTEX archive has an unexpected top-level layout".into()));
    }
    Ok(entries.swap_remove(0).path())
}

fn initialize_postgres(staging: &Path, socket: &Path, port: u16) -> Result<(), Failure> {
    let cluster = staging.join("postgres");
    let password = random_password()?;
    let password_file = staging.join("postgres-password.tmp");
    publish_bytes(&password_file, password.as_bytes())?;
    private_file(&password_file)?;
    let status = Command::new(executable("initdb")?)
        .arg("--pgdata")
        .arg(&cluster)
        .arg("--username=artex")
        .arg("--auth-host=scram-sha-256")
        .arg("--auth-local=scram-sha-256")
        .arg("--no-instructions")
        .arg("--pwfile")
        .arg(&password_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(command_failed("initialize ARTEX PostgreSQL"))?;
    fs::remove_file(&password_file).map_err(io_failed)?;
    if !status.success() {
        return Err(Failure::Failed("ARTEX PostgreSQL initialization failed".into()));
    }
    fs::create_dir_all(socket).map_err(io_failed)?;
    let mut configuration =
        OpenOptions::new().append(true).open(cluster.join("postgresql.conf")).map_err(io_failed)?;
    writeln!(
        configuration,
        "\nport = {port}\nlisten_addresses = '127.0.0.1'\nunix_socket_directories = '{}'\n",
        pg_quote(socket)?
    )
    .map_err(io_failed)?;
    publish_bytes(&staging.join("postgres-password"), password.as_bytes())?;
    private_file(&staging.join("postgres-password"))?;
    start_postgres(&cluster)?;
    let mut createdb = Command::new(executable("createdb")?);
    let created = createdb
        .args(["--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .args(["--username", "artex", "artex"])
        .env("PGPASSWORD", &password)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(command_failed("create the ARTEX PostgreSQL database"))?;
    let stopped = stop_postgres(&cluster);
    if !created.success() {
        return Err(Failure::Failed("ARTEX PostgreSQL database creation failed".into()));
    }
    stopped
}

fn write_config(staging: &Path, final_root: &Path, port: u16) -> Result<(), Failure> {
    let password = fs::read_to_string(staging.join("postgres-password")).map_err(io_failed)?;
    let configuration = json!({
        "database":{
            "host":"127.0.0.1",
            "port":port,
            "user":"artex",
            "password":password,
            "dbname":"artex",
            "sslmode":"disable",
        },
        "skill_dir":final_root.join("app").join("skills"),
    });
    let path = staging.join("app").join("config.json");
    publish_bytes(&path, &serde_json::to_vec_pretty(&configuration).map_err(json_failed)?)?;
    private_file(&path)
}

fn start_postgres(cluster: &Path) -> Result<(), Failure> {
    if postgres_running(cluster) {
        return Ok(());
    }
    let status = Command::new(executable("pg_ctl")?)
        .arg("-D")
        .arg(cluster)
        .arg("-l")
        .arg(cluster.join("postgres.log"))
        .args(["start", "-w", "-t", "30"])
        .status()
        .map_err(command_failed("start ARTEX PostgreSQL"))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Failed("ARTEX PostgreSQL did not start".into()))
    }
}

fn stop_postgres(cluster: &Path) -> Result<(), Failure> {
    if !cluster.is_dir() || !postgres_running(cluster) {
        return Ok(());
    }
    let status = Command::new(executable("pg_ctl")?)
        .arg("-D")
        .arg(cluster)
        .args(["stop", "-m", "fast", "-w", "-t", "30"])
        .status()
        .map_err(command_failed("stop ARTEX PostgreSQL"))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Failed("ARTEX PostgreSQL did not stop cleanly".into()))
    }
}

fn postgres_running(cluster: &Path) -> bool {
    cluster.is_dir()
        && executable("pg_ctl").is_ok_and(|pg_ctl| {
            Command::new(pg_ctl)
                .arg("-D")
                .arg(cluster)
                .arg("status")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
}

fn health(port: u16) -> bool {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .and_then(|client| client.get(format!("http://127.0.0.1:{port}/api/health")).send())
        .is_ok_and(|response| response.status().is_success())
}

fn available_postgres_port() -> Result<u16, Failure> {
    (FIRST_POSTGRES_PORT..=LAST_POSTGRES_PORT)
        .find(|port| port_available(*port))
        .ok_or_else(|| Failure::Rejected("no private loopback PostgreSQL port is available".into()))
}

fn port_available(port: u16) -> bool {
    TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)).is_ok()
}

fn process_owned(pid: u32, application: &Path) -> bool {
    let process = PathBuf::from("/proc").join(pid.to_string());
    fs::canonicalize(process.join("cwd")).is_ok_and(|cwd| cwd == application)
        && fs::read(process.join("cmdline"))
            .is_ok_and(|bytes| bytes.windows("start.sh".len()).any(|window| window == b"start.sh"))
}

fn random_password() -> Result<String, Failure> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| Failure::Failed(error.to_string()))?;
    Ok(crate::encode_hex(&bytes))
}

fn pg_quote(path: &Path) -> Result<String, Failure> {
    let value =
        path.to_str().ok_or_else(|| Failure::Rejected("ARTEX state path is not UTF-8".into()))?;
    if value.chars().any(|character| matches!(character, '\'' | '\n' | '\r' | '\0')) {
        return Err(Failure::Rejected("ARTEX state path is unsafe for PostgreSQL".into()));
    }
    Ok(value.into())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, Failure> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(json_failed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_failed(error)),
    }
}

fn publish_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Failure> {
    publish_bytes(path, &serde_json::to_vec_pretty(value).map_err(json_failed)?)
}

fn publish_bytes(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let parent =
        path.parent().ok_or_else(|| Failure::Rejected("state path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(io_failed)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_failed)?;
    temporary.write_all(bytes).map_err(io_failed)?;
    temporary.as_file().sync_all().map_err(io_failed)?;
    temporary.persist(path).map_err(|error| io_failed(error.error))?;
    Ok(())
}

fn private_directory(path: &Path) -> Result<(), Failure> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(io_failed)
}

fn private_file(path: &Path) -> Result<(), Failure> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io_failed)
}

fn now() -> Result<u64, Failure> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| Failure::Failed("system clock is before the Unix epoch".into()))
}

fn preparation_failed(error: preparation::Failure) -> Failure {
    match error {
        preparation::Failure::Cancelled => Failure::Failed("ARTEX download was cancelled".into()),
        preparation::Failure::Rejected(detail) => Failure::Rejected(detail),
        preparation::Failure::Infrastructure(detail) => Failure::Failed(detail),
    }
}

fn command_failed(operation: &'static str) -> impl FnOnce(std::io::Error) -> Failure {
    move |error| Failure::Failed(format!("could not {operation}: {error}"))
}

fn io_failed(error: impl std::fmt::Display) -> Failure {
    Failure::Failed(error.to_string())
}

fn json_failed(error: impl std::fmt::Display) -> Failure {
    Failure::Rejected(format!("invalid retained ARTEX state: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_release_and_license_have_exact_public_identities() {
        let (url, digest, architecture) = release().unwrap();
        assert!(url.starts_with("https://github.com/BaaRai/ARTEX/releases/download/v0.5.1/"));
        assert_eq!(digest.len(), 64);
        assert!(matches!(architecture, "amd64" | "arm64"));
        assert_eq!(LICENSE_EXPRESSION, "AGPL-3.0-only");
        assert_eq!(LICENSE_SHA256.len(), 64);
    }

    #[test]
    fn postgres_configuration_rejects_injectable_paths() {
        assert!(pg_quote(Path::new("/tmp/ordinary socket")).is_ok());
        assert!(pg_quote(Path::new("/tmp/bad'path")).is_err());
        assert!(pg_quote(Path::new("/tmp/bad\npath")).is_err());
    }

    #[test]
    #[ignore = "downloads the pinned upstream release and starts private PostgreSQL and ARTEX"]
    fn pinned_release_runs_with_private_postgres_and_loopback_health() {
        let temporary = tempfile::tempdir().unwrap();
        let staging = temporary.path();
        let (url, digest, _) = release().unwrap();
        let fetched = preparation::fetch(
            &preparation::Specification {
                url: url.into(),
                expected_sha256: Some(digest.into()),
                maximum_bytes: MAX_ARCHIVE_BYTES,
            },
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap_or_else(|failure| panic!("release download failed: {}", failure_detail(failure)));
        assert_eq!(fetched.digest, digest);
        let archive = staging.join("artex.zip");
        publish_bytes(&archive, &fetched.bytes).unwrap();
        let extracted = staging.join("extracted");
        extract_archive(&archive, &extracted).unwrap();
        let application = staging.join("app");
        fs::rename(extracted_root(&extracted).unwrap(), &application).unwrap();
        assert!(Command::new(application.join("artex")).arg("-h").status().unwrap().success());

        let postgres_port = available_postgres_port().unwrap();
        initialize_postgres(staging, &staging.join("socket"), postgres_port).unwrap();
        write_config(staging, staging, postgres_port).unwrap();
        fs::create_dir_all(staging.join("data")).unwrap();
        fs::create_dir_all(staging.join("keys")).unwrap();
        start_postgres(&staging.join("postgres")).unwrap();

        let application_port = (58_787..58_887).find(|port| port_available(*port)).unwrap();
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(staging.join("artex-test.log"))
            .unwrap();
        let stderr = log.try_clone().unwrap();
        let mut child = Command::new(application.join("artex"))
            .args(["-addr", &format!("127.0.0.1:{application_port}"), "-proxy", ""])
            .arg("-data")
            .arg(staging.join("data"))
            .current_dir(&application)
            .env_clear()
            .env("ARTEX_CONFIG", application.join("config.json"))
            .env("ARTEX_JWT_KEY_DIR", staging.join("keys"))
            .env("HOME", env::var_os("HOME").unwrap_or_default())
            .env("PATH", env::var_os("PATH").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let mut ready = false;
        for _ in 0..100 {
            if health(application_port) {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(ready, "ARTEX did not become healthy; see temporary test log");
        assert!(
            Command::new(executable("kill").unwrap())
                .args(["-TERM", &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        assert!(child.wait().unwrap().success());
        stop_postgres(&staging.join("postgres")).unwrap();
        assert!(!postgres_running(&staging.join("postgres")));
    }

    fn failure_detail(failure: preparation::Failure) -> String {
        match failure {
            preparation::Failure::Cancelled => "cancelled".into(),
            preparation::Failure::Rejected(detail)
            | preparation::Failure::Infrastructure(detail) => detail,
        }
    }
}
