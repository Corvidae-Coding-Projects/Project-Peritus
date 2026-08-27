//! Checked-in H0 threat, control, inventory, and schema assets.

/// One source-controlled security qualification asset.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BundledSecurityAsset {
    path: &'static str,
    contents: &'static str,
}

impl BundledSecurityAsset {
    /// Returns the repository-relative path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        self.path
    }

    /// Returns exact UTF-8 contents compiled into the crate.
    #[must_use]
    pub const fn contents(self) -> &'static str {
        self.contents
    }
}

/// Returns every H0-owned catalog and schema compiled into the qualification crate.
#[must_use]
pub const fn bundled_security_assets() -> &'static [BundledSecurityAsset] {
    &ASSETS
}

const ASSETS: [BundledSecurityAsset; 7] = [
    BundledSecurityAsset {
        path: "security/threat-model-v1.toml",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/threat-model-v1.toml"
        )),
    },
    BundledSecurityAsset {
        path: "security/control-catalog-v1.toml",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/control-catalog-v1.toml"
        )),
    },
    BundledSecurityAsset {
        path: "security/unsafe-inventory-v1.toml",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/unsafe-inventory-v1.toml"
        )),
    },
    BundledSecurityAsset {
        path: "security/tcb-inventory-v1.toml",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/tcb-inventory-v1.toml"
        )),
    },
    BundledSecurityAsset {
        path: "security/schemas/evidence-manifest-v1.schema.json",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/schemas/evidence-manifest-v1.schema.json"
        )),
    },
    BundledSecurityAsset {
        path: "security/schemas/external-review-v1.schema.json",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/schemas/external-review-v1.schema.json"
        )),
    },
    BundledSecurityAsset {
        path: "security/schemas/control-catalog-v1.schema.json",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../security/schemas/control-catalog-v1.schema.json"
        )),
    },
];
