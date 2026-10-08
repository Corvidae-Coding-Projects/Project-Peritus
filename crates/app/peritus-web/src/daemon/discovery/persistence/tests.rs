use super::*;
use crate::config::Options;
use peritus_product_state::{BootstrapPhase, InstallIdentity};

fn product(store: u8, generation: u64) -> Vec<u8> {
    let mut state = ProductState::new(
        InstallIdentity::new([store; 16], [9; 16]).expect("installation identity"),
    );
    if generation == 2 {
        state
            .advance(BootstrapPhase::RegistryReady)
            .expect("second generation");
    } else {
        assert_eq!(generation, 1);
    }
    state.canonical_json().expect("product state")
}

fn config(store: u8, state_root: &Path) -> Vec<u8> {
    format!(
        "version = 1\nstore_id = {:?}\n[paths]\nstate_root = {:?}\n",
        format!("{store:02x}").repeat(16),
        state_root.to_string_lossy()
    )
    .into_bytes()
}

fn app(root: &Path, explicit: Option<PathBuf>, endpoint: Option<PathBuf>) -> App {
    App::open(
        Options {
            port: 4173,
            root: root.to_owned(),
            assets: root.join("assets"),
            config_file: root.join("webui.toml"),
            state_file: root.join("web/workspace.json"),
            daemon_config_root: root.join("config"),
            product_state_root: root.join("product-state"),
            daemon_config: explicit,
            endpoint,
            cli: PathBuf::from("peritus"),
        },
        4173,
    )
    .expect("application")
}

fn roots() -> (tempfile::TempDir, PathBuf) {
    let temporary = tempfile::tempdir().expect("root");
    let root = temporary.path().canonicalize().expect("canonical root");
    std::fs::create_dir(root.join("config")).expect("configuration root");
    std::fs::create_dir(root.join("product-state")).expect("product root");
    (temporary, root)
}

#[test]
fn discovery_selects_the_latest_coherent_generation_in_one_store() {
    let (_temporary, root) = roots();
    std::fs::write(root.join("config/peritus-1.toml"), config(1, &root))
        .expect("first configuration");
    std::fs::write(root.join("product-state/state-1.json"), product(1, 1))
        .expect("first product state");
    std::fs::write(root.join("config/peritus-2.toml"), config(2, &root))
        .expect("conflicting configuration");
    std::fs::write(root.join("product-state/state-2.json"), product(1, 2))
        .expect("second product state");

    let target = discover(&app(&root, None, None)).expect("coherent target").target;
    assert_eq!(target.generation, 1);
    assert_eq!(target.store, "01".repeat(16));
}

#[test]
fn explicit_config_accepts_an_arbitrary_filename_and_matching_store() {
    let (_temporary, root) = roots();
    let explicit = root.join("peritus.toml");
    std::fs::write(&explicit, config(1, &root)).expect("configuration");
    std::fs::write(root.join("product-state/state-1.json"), product(2, 1))
        .expect("other store");
    std::fs::write(root.join("product-state/state-2.json"), product(1, 2))
        .expect("matching store");

    let target = discover(&app(&root, Some(explicit), None))
        .expect("coherent target")
        .target;
    assert_eq!(target.generation, 2);
    assert_eq!(target.store, "01".repeat(16));
}

#[test]
fn retained_target_survives_source_replacement_and_deletion() {
    let (_temporary, root) = roots();
    let config_path = root.join("config/peritus-1.toml");
    let product_path = root.join("product-state/state-1.json");
    std::fs::write(&config_path, config(1, &root)).expect("configuration");
    std::fs::write(&product_path, product(1, 1)).expect("product state");
    let target = discover(&app(&root, None, None)).expect("coherent target").target;

    std::fs::write(&config_path, config(2, &root)).expect("replacement");
    std::fs::remove_file(product_path).expect("delete source");

    assert_eq!(verify_target(&target).expect("retained target").target, target);
    assert!(target.config.ends_with("config.toml"));
    assert!(target.product_state.ends_with("product-state.json"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&target.config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&target.product_state)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn retained_target_rejects_snapshot_mutation() {
    let (_temporary, root) = roots();
    std::fs::write(root.join("config/peritus-1.toml"), config(1, &root))
        .expect("configuration");
    std::fs::write(root.join("product-state/state-1.json"), product(1, 1))
        .expect("product state");
    let target = discover(&app(&root, None, None)).expect("coherent target").target;

    std::fs::write(&target.config, config(2, &root)).expect("mutated snapshot");
    let error = verify_target(&target).err().expect("snapshot mutation");
    assert!(error.0.contains("changed in place"));
}

#[test]
fn explicit_canonical_endpoint_is_accepted() {
    let (_temporary, root) = roots();
    let config = config(1, &root);
    let (parsed, store) = configuration(&config).expect("configuration");
    let endpoint = endpoint_from_configuration(&parsed, &store).expect("endpoint");
    std::fs::write(root.join("config/peritus-1.toml"), config).expect("configuration");
    std::fs::write(root.join("product-state/state-1.json"), product(1, 1))
        .expect("product state");

    assert!(discover(&app(&root, None, Some(endpoint))).is_ok());
}

#[test]
fn explicit_endpoint_must_be_the_address_derived_from_the_store() {
    let (_temporary, root) = roots();
    std::fs::write(root.join("config/peritus-1.toml"), config(1, &root))
        .expect("configuration");
    std::fs::write(root.join("product-state/state-1.json"), product(1, 1))
        .expect("product state");
    let error = discover(&app(&root, None, Some(root.join("unrelated.sock"))))
        .err()
        .expect("mismatched endpoint");
    assert!(error.0.contains("does not belong to the selected store"));
}
