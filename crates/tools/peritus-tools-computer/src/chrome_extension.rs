//! Private unpacked Chrome extension installation and native-message bridge.

use std::{
    env,
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{Value, json};

const HOST_NAME: &str = "com.corvidae.peritus";
const EXTENSION_ID: &str = "bddedfabmnahjhfenoofaljnjkcpllmh";
const MAX_NATIVE_MESSAGE: usize = 4 * 1024 * 1024;
const MAX_LOCAL_REQUEST: usize = 128 * 1024;

const MANIFEST: &str = include_str!("../extension/manifest.json");
const SERVICE_WORKER: &str = include_str!("../extension/service-worker.js");
const POPUP_HTML: &str = include_str!("../extension/popup.html");
const POPUP_JS: &str = include_str!("../extension/popup.js");

/// Stable Chrome extension identifier derived from the bundled public manifest key.
pub const CHROME_EXTENSION_ID: &str = EXTENSION_ID;

/// Returns true when the executable was invoked through the dedicated native-host link.
#[must_use]
pub fn native_host_requested() -> bool {
    env::args_os()
        .next()
        .and_then(|path| PathBuf::from(path).file_name().map(ToOwned::to_owned))
        .is_some_and(|name| name == "peritus-browser-host")
}

fn root() -> Result<PathBuf, String> {
    Ok(super::installation_root()?.join("chrome-extension"))
}

/// Private directory containing the unpacked extension Chrome should load once.
///
/// # Errors
///
/// Returns an error when the current user's data directory cannot be resolved.
pub fn unpacked_directory() -> Result<PathBuf, String> {
    Ok(root()?.join("unpacked"))
}

/// Private local bridge socket consumed by run-owned computer controllers.
///
/// # Errors
///
/// Returns an error when the current user's data directory cannot be resolved.
pub fn bridge_socket() -> Result<PathBuf, String> {
    Ok(root()?.join("native.sock"))
}

/// Private shared token file consumed by the native host and run-owned controller.
///
/// # Errors
///
/// Returns an error when the current user's data directory cannot be resolved.
pub fn bridge_token_file() -> Result<PathBuf, String> {
    Ok(root()?.join("bridge-token"))
}

fn set_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("protect {}: {error}", path.display()))?;
    }
    Ok(())
}

fn write_private(path: &Path, contents: &[u8]) -> Result<(), String> {
    if path.is_symlink() {
        return Err(format!("refusing symbolic-link extension path: {}", path.display()));
    }
    let parent = path.parent().ok_or("extension file has no parent directory")?;
    set_private_directory(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("prepare {}: {error}", path.display()))?;
    temporary
        .write_all(contents)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("protect {}: {error}", path.display()))?;
    }
    temporary
        .persist(path)
        .map_err(|error| format!("publish {}: {}", path.display(), error.error))?;
    Ok(())
}

fn read_token() -> Result<String, String> {
    let path = bridge_token_file()?;
    if path.is_symlink() {
        return Err("browser bridge token path is a symbolic link".into());
    }
    let value = fs::read_to_string(&path).map_err(|error| {
        format!(
            "read browser bridge token: {error}; run peritus automation chrome-extension install"
        )
    })?;
    let value = value.trim();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(value.to_owned());
    }
    Err("browser bridge token is malformed; remove it and reinstall explicitly".into())
}

fn ensure_token() -> Result<String, String> {
    if bridge_token_file()?.is_file() {
        return read_token();
    }
    let path = bridge_token_file()?;
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("generate browser bridge token: {error}"))?;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(value, "{byte:02x}").map_err(|error| error.to_string())?;
    }
    write_private(&path, format!("{value}\n").as_bytes())?;
    Ok(value)
}

/// Returns the socket and private token-file paths for a run-owned controller.
///
/// # Errors
///
/// Returns an error when installation paths are unavailable or the token is absent or malformed.
pub fn bridge_environment() -> Result<(PathBuf, PathBuf), String> {
    let token_file = bridge_token_file()?;
    let _ = read_token()?;
    Ok((bridge_socket()?, token_file))
}

fn browser_manifest_directories() -> Result<Vec<PathBuf>, String> {
    let home = env::var_os("HOME").map(PathBuf::from).ok_or("HOME is unavailable")?;
    #[cfg(target_os = "macos")]
    let roots = vec![
        home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts"),
        home.join("Library/Application Support/Chromium/NativeMessagingHosts"),
    ];
    #[cfg(not(target_os = "macos"))]
    let mut roots = {
        let home_config = home.join(".config");
        let xdg_config =
            env::var_os("XDG_CONFIG_HOME").map_or_else(|| home_config.clone(), PathBuf::from);
        vec![
            home_config.join("google-chrome/NativeMessagingHosts"),
            home_config.join("google-chrome-beta/NativeMessagingHosts"),
            home_config.join("google-chrome-unstable/NativeMessagingHosts"),
            home_config.join("google-chrome-for-testing/NativeMessagingHosts"),
            home_config.join("chromium/NativeMessagingHosts"),
            xdg_config.join("google-chrome/NativeMessagingHosts"),
            xdg_config.join("google-chrome-beta/NativeMessagingHosts"),
            xdg_config.join("google-chrome-unstable/NativeMessagingHosts"),
            xdg_config.join("google-chrome-for-testing/NativeMessagingHosts"),
            xdg_config.join("chromium/NativeMessagingHosts"),
        ]
    };
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn native_link() -> Result<PathBuf, String> {
    Ok(root()?.join("peritus-browser-host"))
}

#[cfg(unix)]
fn install_native_link(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let executable = env::current_exe().map_err(|error| format!("locate peritus: {error}"))?;
    if path.exists() || path.is_symlink() {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("inspect {}: {error}", path.display()))?;
        if !metadata.file_type().is_symlink() {
            return Err(format!(
                "native-host path is not a managed symbolic link: {}",
                path.display()
            ));
        }
        fs::remove_file(path).map_err(|error| format!("replace {}: {error}", path.display()))?;
    }
    symlink(&executable, path)
        .map_err(|error| format!("link {} to {}: {error}", path.display(), executable.display()))
}

#[cfg(not(unix))]
fn install_native_link(_path: &Path) -> Result<(), String> {
    Err("the private Chrome extension native host is currently implemented for Linux/macOS".into())
}

/// Installs exact private extension sources and per-user Chrome native-host registration.
///
/// # Errors
///
/// Returns an error when private assets, the executable link, or browser registrations cannot be
/// validated and written for the current user.
pub fn install() -> Result<Value, String> {
    let extension_root = root()?;
    let unpacked = unpacked_directory()?;
    set_private_directory(&extension_root)?;
    set_private_directory(&unpacked)?;
    for (name, contents) in [
        ("manifest.json", MANIFEST),
        ("service-worker.js", SERVICE_WORKER),
        ("popup.html", POPUP_HTML),
        ("popup.js", POPUP_JS),
    ] {
        write_private(&unpacked.join(name), contents.as_bytes())?;
    }
    let _ = ensure_token()?;
    let link = native_link()?;
    install_native_link(&link)?;
    let native_manifest = serde_json::to_vec_pretty(&json!({
        "name": HOST_NAME,
        "description": "Private Peritus bridge for loopback control and selected HTTPS reads",
        "path": link,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")]
    }))
    .map_err(|error| format!("encode Chrome native-host manifest: {error}"))?;
    let mut registrations = Vec::new();
    for directory in browser_manifest_directories()? {
        let path = directory.join(format!("{HOST_NAME}.json"));
        write_private(&path, &native_manifest)?;
        registrations.push(path);
    }
    Ok(json!({
        "success": true,
        "private": true,
        "published": false,
        "extension_id": EXTENSION_ID,
        "unpacked_directory": unpacked,
        "native_host": link,
        "registrations": registrations,
        "next": [
            "Open chrome://extensions",
            "Enable Developer mode",
            "Choose Load unpacked and select unpacked_directory once"
        ]
    }))
}

fn registration_state() -> Result<Vec<Value>, String> {
    let expected = native_link()?;
    browser_manifest_directories()?
        .into_iter()
        .map(|directory| {
            let path = directory.join(format!("{HOST_NAME}.json"));
            let parsed = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            Ok(json!({
                "path": path,
                "present": path.is_file() && !path.is_symlink(),
                "points_to_native_host": parsed.as_ref().and_then(|value| value["path"].as_str())
                    == Some(expected.to_string_lossy().as_ref()),
                "allows_only_extension": parsed.as_ref().and_then(|value| value["allowed_origins"].as_array())
                    .is_some_and(|origins| origins == &[json!(format!("chrome-extension://{EXTENSION_ID}/"))])
            }))
        })
        .collect()
}

/// Reports installed file and live bridge state without starting Chrome.
///
/// # Errors
///
/// Returns an error when the current user's installation or browser registration paths cannot be
/// resolved.
pub fn status() -> Result<Value, String> {
    let unpacked = unpacked_directory()?;
    let assets = ["manifest.json", "service-worker.js", "popup.html", "popup.js"]
        .into_iter()
        .map(|name| {
            let path = unpacked.join(name);
            let expected = match name {
                "manifest.json" => MANIFEST,
                "service-worker.js" => SERVICE_WORKER,
                "popup.html" => POPUP_HTML,
                _ => POPUP_JS,
            };
            json!({"name":name,"present":path.is_file() && !path.is_symlink(),"current":fs::read(&path).is_ok_and(|bytes| bytes == expected.as_bytes())})
        })
        .collect::<Vec<_>>();
    let live = bridge_request(&json!({"action":"status"})).ok();
    Ok(json!({
        "success": true,
        "private": true,
        "published": false,
        "extension_id": EXTENSION_ID,
        "unpacked_directory": unpacked,
        "assets": assets,
        "registrations": registration_state()?,
        "native_host_link": native_link()?,
        "native_host_link_present": native_link()?.is_symlink(),
        "bridge_socket": bridge_socket()?,
        "bridge_connected": live.as_ref().and_then(|value| value["success"].as_bool()) == Some(true),
        "bridge": live,
        "setup": "peritus automation chrome-extension install"
    }))
}

/// Removes only Peritus's private registration and unpacked extension files.
///
/// # Errors
///
/// Returns an error when an exact managed registration or extension asset cannot be removed.
pub fn uninstall() -> Result<Value, String> {
    let mut removed = Vec::new();
    for directory in browser_manifest_directories()? {
        let path = directory.join(format!("{HOST_NAME}.json"));
        if path.is_file() && !path.is_symlink() {
            fs::remove_file(&path)
                .map_err(|error| format!("remove {}: {error}", path.display()))?;
            removed.push(path);
        }
    }
    let extension_root = root()?;
    if extension_root.is_symlink() {
        return Err("refusing symbolic-link Chrome extension root".into());
    }
    if extension_root.is_dir() {
        fs::remove_dir_all(&extension_root)
            .map_err(|error| format!("remove {}: {error}", extension_root.display()))?;
        removed.push(extension_root);
    }
    Ok(
        json!({"success":true,"removed":removed,"chrome_action":"Remove the unpacked extension from chrome://extensions if it is still listed"}),
    )
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right).fold(0_u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

#[cfg(unix)]
fn bridge_request(request: &Value) -> Result<Value, String> {
    use std::os::unix::net::UnixStream;
    let mut payload = request.clone();
    payload
        .as_object_mut()
        .ok_or("Chrome bridge request must be a JSON object")?
        .insert("_token".into(), Value::String(read_token()?));
    let mut encoded = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_LOCAL_REQUEST {
        return Err("Chrome bridge request exceeds its byte limit".into());
    }
    encoded.push(b'\n');
    let mut stream = UnixStream::connect(bridge_socket()?)
        .map_err(|error| format!("Chrome extension bridge is unavailable: {error}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(45))).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|error| error.to_string())?;
    stream.write_all(&encoded).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    BufReader::new(stream)
        .take((MAX_NATIVE_MESSAGE + 1) as u64)
        .read_until(b'\n', &mut response)
        .map_err(|error| error.to_string())?;
    if response.len() > MAX_NATIVE_MESSAGE || !response.ends_with(b"\n") {
        return Err("Chrome bridge response was oversized or interrupted".into());
    }
    serde_json::from_slice(&response)
        .map_err(|error| format!("decode Chrome bridge response: {error}"))
}

#[cfg(not(unix))]
fn bridge_request(_request: &Value) -> Result<Value, String> {
    Err("the private Chrome extension bridge is currently implemented for Linux/macOS".into())
}

fn read_native_message(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| format!("read native message length: {error}"))?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_NATIVE_MESSAGE {
        return Err("Chrome native message exceeds its byte limit".into());
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload).map_err(|error| format!("read native message: {error}"))?;
    Ok(payload)
}

fn write_native_message(writer: &mut impl Write, payload: &[u8]) -> Result<(), String> {
    let length = u32::try_from(payload.len()).map_err(|_| "Chrome native message is too large")?;
    writer
        .write_all(&length.to_le_bytes())
        .and_then(|()| writer.write_all(payload))
        .and_then(|()| writer.flush())
        .map_err(|error| format!("write Chrome native message: {error}"))
}

#[cfg(unix)]
fn prepare_listener(path: &Path) -> Result<std::os::unix::net::UnixListener, String> {
    use std::os::unix::{fs::FileTypeExt, fs::PermissionsExt, net::UnixStream};
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err("another Peritus Chrome native host is already connected".into());
        }
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("inspect stale bridge socket: {error}"))?;
        if !metadata.file_type().is_socket() {
            return Err("Chrome bridge path exists and is not a socket".into());
        }
        fs::remove_file(path).map_err(|error| format!("remove stale bridge socket: {error}"))?;
    }
    let listener = std::os::unix::net::UnixListener::bind(path)
        .map_err(|error| format!("bind Chrome bridge socket: {error}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("protect Chrome bridge socket: {error}"))?;
    Ok(listener)
}

#[cfg(unix)]
struct SocketCleanup(PathBuf);

#[cfg(unix)]
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Runs the Chrome native-messaging relay. Invoke only through `peritus-browser-host`.
///
/// # Errors
///
/// Returns an error for the wrong extension origin, unavailable private state, malformed or
/// oversized messages, transport failures, and mismatched extension responses.
pub fn run_native_host() -> Result<(), String> {
    #[cfg(not(unix))]
    return Err(
        "the private Chrome extension native host is currently implemented for Linux/macOS".into(),
    );

    #[cfg(unix)]
    {
        let origin = env::args().nth(1).ok_or("Chrome did not provide an extension origin")?;
        let expected = format!("chrome-extension://{EXTENSION_ID}/");
        if origin != expected {
            return Err("Chrome native-host origin does not match the bundled extension".into());
        }
        let root = root()?;
        set_private_directory(&root)?;
        let socket = bridge_socket()?;
        let listener = prepare_listener(&socket)?;
        let _cleanup = SocketCleanup(socket);
        let expected_token = read_token()?;
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut native_reader = stdin.lock();
        let mut native_writer = stdout.lock();
        let mut sequence = 0_u64;
        for connection in listener.incoming() {
            let mut connection =
                connection.map_err(|error| format!("accept Chrome bridge client: {error}"))?;
            connection
                .set_read_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
            connection
                .set_write_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
            let outcome = (|| -> Result<Value, String> {
                let mut line = Vec::new();
                BufReader::new(&connection)
                    .take((MAX_LOCAL_REQUEST + 1) as u64)
                    .read_until(b'\n', &mut line)
                    .map_err(|error| format!("read Chrome bridge request: {error}"))?;
                if line.len() > MAX_LOCAL_REQUEST || !line.ends_with(b"\n") {
                    return Err("Chrome bridge request exceeds its byte limit".into());
                }
                let mut request: Value = serde_json::from_slice(&line)
                    .map_err(|error| format!("decode Chrome bridge request: {error}"))?;
                let object =
                    request.as_object_mut().ok_or("Chrome bridge request must be an object")?;
                let supplied = object
                    .remove("_token")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or("Chrome bridge request has no authentication token")?;
                if !constant_time_equal(supplied.as_bytes(), expected_token.as_bytes()) {
                    return Err("Chrome bridge authentication failed".into());
                }
                sequence = sequence.wrapping_add(1);
                let bridge_id = format!("{}-{sequence}", std::process::id());
                object.insert("_bridge_id".into(), Value::String(bridge_id.clone()));
                let encoded = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
                write_native_message(&mut native_writer, &encoded)?;
                let response = read_native_message(&mut native_reader)?;
                let mut value: Value = serde_json::from_slice(&response)
                    .map_err(|error| format!("decode extension response: {error}"))?;
                if value["_bridge_id"].as_str() != Some(bridge_id.as_str()) {
                    return Err("Chrome extension returned a mismatched bridge response".into());
                }
                if let Some(object) = value.as_object_mut() {
                    object.remove("_bridge_id");
                }
                Ok(value)
            })();
            let response = outcome.unwrap_or_else(|error| json!({"success":false,"error":error}));
            let mut encoded = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
            if encoded.len() > MAX_NATIVE_MESSAGE {
                encoded = serde_json::to_vec(&json!({"success":false,"error":"Chrome extension response exceeds its byte limit"})).map_err(|error| error.to_string())?;
            }
            encoded.push(b'\n');
            connection
                .write_all(&encoded)
                .map_err(|error| format!("write Chrome bridge response: {error}"))?;
        }
        Ok(())
    }
}
