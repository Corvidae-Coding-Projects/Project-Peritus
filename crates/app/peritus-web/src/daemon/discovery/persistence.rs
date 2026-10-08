//! Discovery and immutable retention of exact native generations.

use super::{Discovery, NativeTarget};
use crate::{
    daemon::{bytes, hex},
    error::{Result, problem},
    state::App,
};
use peritus_product_state::ProductState;
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

mod retention;
use retention::{publish_exact, secure_directory, sync_directory};

pub(super) fn discover(app: &App) -> Result<Discovery> {
    discover_options(&app.options)
}

pub(super) fn discover_options(options: &crate::config::Options) -> Result<Discovery> {
    let states = generations(&options.product_state_root, "state-", ".json")?;
    if let Some(config) = &options.daemon_config {
        return discover_explicit(options, config, &states);
    }
    let configurations = generations(&options.daemon_config_root, "peritus-", ".toml")?;
    let mut rejected = Vec::new();
    for (generation, config_path) in configurations.into_iter().rev() {
        let Some(product_path) = states.get(&generation) else {
            rejected.push(format!("generation {generation} has no matching product state"));
            continue;
        };
        match read_and_decode(
            generation,
            &config_path,
            product_path,
            options.endpoint.as_deref(),
        ) {
            Ok((discovery, config, product)) => {
                return retain(options, discovery, &config, &product);
            }
            Err(error) => rejected.push(format!("generation {generation}: {}", error.0)),
        }
    }
    no_generation(rejected)
}

pub(super) fn verify_target(target: &NativeTarget) -> Result<Discovery> {
    let config = read_exact(&target.config, &target.config_sha256, "daemon configuration")?;
    let product = read_exact(
        &target.product_state,
        &target.product_state_sha256,
        "product state",
    )?;
    let discovery = decode_pair(
        target.generation,
        target.config.clone(),
        &config,
        target.product_state.clone(),
        &product,
        Some(&target.endpoint),
    )?;
    if discovery.target != *target {
        return Err(problem(
            "The retained native target no longer has the same identity",
        ));
    }
    Ok(discovery)
}

fn discover_explicit(
    options: &crate::config::Options,
    config_path: &Path,
    states: &BTreeMap<u64, PathBuf>,
) -> Result<Discovery> {
    let config_path = exact_source(config_path, "daemon configuration")?;
    let config = std::fs::read(&config_path)?;
    let expected_store = configuration(&config)?.1;
    let mut rejected = Vec::new();
    for (generation, product_path) in states.iter().rev() {
        let product_path = match exact_source(product_path, "product state") {
            Ok(path) => path,
            Err(error) => {
                rejected.push(format!("generation {generation}: {}", error.0));
                continue;
            }
        };
        let product = match std::fs::read(&product_path) {
            Ok(product) => product,
            Err(error) => {
                rejected.push(format!("generation {generation}: {error}"));
                continue;
            }
        };
        let state = match ProductState::parse_json(&product) {
            Ok(state) => state,
            Err(error) => {
                rejected.push(format!("generation {generation}: {error}"));
                continue;
            }
        };
        if state.generation() != *generation {
            rejected.push(format!(
                "generation {generation}: product-state payload and filename generations differ"
            ));
            continue;
        }
        if state.identity().store_id() != expected_store {
            continue;
        }
        let discovery = decode_pair(
            *generation,
            config_path.clone(),
            &config,
            product_path,
            &product,
            options.endpoint.as_deref(),
        )?;
        return retain(options, discovery, &config, &product);
    }
    no_generation(rejected)
}

fn read_and_decode(
    generation: u64,
    config_path: &Path,
    product_path: &Path,
    endpoint: Option<&Path>,
) -> Result<(Discovery, Vec<u8>, Vec<u8>)> {
    let config_path = exact_source(config_path, "daemon configuration")?;
    let product_path = exact_source(product_path, "product state")?;
    let config = std::fs::read(&config_path)?;
    let product = std::fs::read(&product_path)?;
    let discovery = decode_pair(
        generation,
        config_path,
        &config,
        product_path,
        &product,
        endpoint,
    )?;
    Ok((discovery, config, product))
}

fn decode_pair(
    generation: u64,
    config_path: PathBuf,
    config_bytes: &[u8],
    product_path: PathBuf,
    product_bytes: &[u8],
    selected_endpoint: Option<&Path>,
) -> Result<Discovery> {
    let (configuration, store) = configuration(config_bytes)?;
    let state = ProductState::parse_json(product_bytes).map_err(problem)?;
    if state.generation() != generation {
        return Err(problem(
            "Product-state payload and filename generations differ",
        ));
    }
    if state.identity().store_id() != store {
        return Err(problem(
            "Daemon configuration and product state belong to different stores",
        ));
    }
    let endpoint = endpoint_from_configuration(&configuration, &store)?;
    if selected_endpoint.is_some_and(|selected| selected != endpoint.as_path()) {
        return Err(problem(
            "Explicit daemon endpoint does not belong to the selected store and state root",
        ));
    }
    Ok(Discovery {
        target: NativeTarget {
            generation,
            store,
            config: config_path,
            config_sha256: digest(config_bytes),
            product_state: product_path,
            product_state_sha256: digest(product_bytes),
            endpoint,
        },
        configuration,
        state,
    })
}

fn configuration(data: &[u8]) -> Result<(toml::Value, String)> {
    let configuration: toml::Value =
        toml::from_str(std::str::from_utf8(data).map_err(problem)?).map_err(problem)?;
    let store = configuration.get("store_id")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| problem("Daemon configuration has no store identity"))?
        .to_owned();
    let store_bytes = bytes(&store)?;
    if store_bytes == [0; 16] || hex(&store_bytes) != store {
        return Err(problem(
            "Daemon configuration store identity is not canonical",
        ));
    }
    Ok((configuration, store))
}

fn endpoint_from_configuration(configuration: &toml::Value, store: &str) -> Result<PathBuf> {
    let mut hash = Sha256::new();
    hash.update(b"peritus/daemon-endpoint/v1\0");
    hash.update(bytes(store)?);
    let name = format!("peritus-{}", hex(&hash.finalize()[..16]));
    #[cfg(unix)]
    {
        let root = configuration
            .get("paths")
            .and_then(|value| value.get("state_root"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| problem("Daemon state directory unavailable"))?;
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(problem("Daemon state directory is not absolute"));
        }
        peritus_local_socket::bounded_path(
            &root.join(format!("{name}.sock")),
            peritus_local_socket::NATIVE_MAX_PATH_BYTES,
        )
        .map_err(problem)
    }
    #[cfg(windows)]
    {
        let _ = configuration;
        Ok(PathBuf::from(format!(r"\\.\pipe\{name}")))
    }
}

fn retain(
    options: &crate::config::Options,
    mut discovery: Discovery,
    config: &[u8],
    product: &[u8],
) -> Result<Discovery> {
    let parent = options.state_file
        .parent()
        .ok_or_else(|| problem("Browser state has no parent directory"))?;
    let root = parent.join("native-targets");
    secure_directory(&root)?;
    let identity = pair_digest(discovery.target.generation, config, product);
    let directory = root.join(format!(
        "{}-{}-{identity}",
        discovery.target.store, discovery.target.generation
    ));
    secure_directory(&directory)?;
    let config_path = directory.join("config.toml");
    let product_path = directory.join("product-state.json");
    publish_exact(&config_path, config)?;
    publish_exact(&product_path, product)?;
    sync_directory(&directory)?;
    sync_directory(&root)?;
    sync_directory(parent)?;
    discovery.target.config = config_path.canonicalize()?;
    discovery.target.product_state = product_path.canonicalize()?;
    Ok(discovery)
}

fn exact_source(path: &Path, kind: &str) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if !std::fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err(problem(format!("The {kind} is not a regular file")));
    }
    Ok(path)
}

fn generations(root: &Path, prefix: &str, suffix: &str) -> Result<BTreeMap<u64, PathBuf>> {
    let mut generations = BTreeMap::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let Some(generation) = parse_generation(&entry.file_name(), prefix, suffix) else {
            continue;
        };
        if generations.insert(generation, entry.path()).is_some() {
            return Err(problem(format!(
                "Multiple immutable files claim generation {generation} under {}",
                root.display()
            )));
        }
    }
    Ok(generations)
}

fn parse_generation(name: &OsStr, prefix: &str, suffix: &str) -> Option<u64> {
    let name = name.to_str()?;
    let digits = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let generation = digits.parse().ok()?;
    (generation > 0).then_some(generation)
}

fn read_exact(path: &Path, expected: &str, kind: &str) -> Result<Vec<u8>> {
    if path.canonicalize()? != path {
        return Err(problem(format!(
            "The retained {kind} path changed identity"
        )));
    }
    let bytes = std::fs::read(path)?;
    if digest(&bytes) != expected {
        return Err(problem(format!(
            "The retained {kind} generation changed in place"
        )));
    }
    Ok(bytes)
}

fn pair_digest(generation: u64, config: &[u8], product: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"peritus/web-native-target/v1\0");
    hash.update(generation.to_le_bytes());
    hash.update(u64::try_from(config.len()).unwrap_or(u64::MAX).to_le_bytes());
    hash.update(config);
    hash.update(u64::try_from(product.len()).unwrap_or(u64::MAX).to_le_bytes());
    hash.update(product);
    hex(&hash.finalize())
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn no_generation<T>(rejected: Vec<String>) -> Result<T> {
    let detail = if rejected.is_empty() {
        String::new()
    } else {
        format!(" ({})", rejected.join("; "))
    };
    Err(problem(format!(
        "Peritus has no coherent daemon configuration and product-state generation{detail}"
    )))
}
#[cfg(test)]
mod tests;
