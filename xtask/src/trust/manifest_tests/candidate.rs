use super::Fixture;

pub(in crate::trust) fn make_cargo_metadata_runnable(fixture: &Fixture) {
    fixture.write(
        "Cargo.toml",
        r#"[workspace]
members = [
    "crates/foundation/peritus-tcb",
    "crates/foundation/peritus-types",
]
resolver = "3"
"#,
    );
    fixture.write(
        "Cargo.lock",
        r#"version = 4

[[package]]
name = "peritus-tcb"
version = "0.0.0"

[[package]]
name = "peritus-types"
version = "0.0.0"
"#,
    );
    for (name, root, class) in [
        ("peritus-tcb", "crates/foundation/peritus-tcb", "T"),
        ("peritus-types", "crates/foundation/peritus-types", "V"),
    ] {
        fixture.write(
            &format!("{root}/Cargo.toml"),
            &format!(
                r#"[package]
name = "{name}"
version = "0.0.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT"

[lib]
path = "src/lib.rs"

[package.metadata.peritus]
owner = "A1"
layer = "foundation"
verification-class = "{class}"
"#,
            ),
        );
    }
}
