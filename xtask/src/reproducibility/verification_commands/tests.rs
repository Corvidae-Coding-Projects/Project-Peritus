use crate::model::ArchitecturePolicy;

mod inventory;

fn policy(packages: &str) -> ArchitecturePolicy {
    toml::from_str(&format!(
        r#"
schema = 3
soft_source_lines = 400
hard_source_lines = 700
root_module_lines = 80
required_license = "MIT"
ignored_directories = []
forbidden_module_names = []
trusted_source_roots = []
source_exceptions = []
layers = []
verification_classes = []
forbidden_dependencies = []
controlled_source_roots = []
{packages}
"#
    ))
    .expect("verification command policy fixture must parse")
}

const CANONICAL_POLICY_PACKAGES: &str = r#"
[[packages]]
name = "peritus-agent"
path = "crates/orchestration/peritus-agent"
owner = "D0"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-app-protocol"
path = "crates/app/peritus-app-protocol"
owner = "A3"
layer = "app"
verification_class = "H"
[[packages]]
name = "peritus-approval"
path = "crates/state/peritus-approval"
owner = "B1"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-artifact-store"
path = "crates/state/peritus-artifact-store"
owner = "C0"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-budget"
path = "crates/foundation/peritus-budget"
owner = "B1"
layer = "foundation"
verification_class = "V"
[[packages]]
name = "peritus-codec"
path = "crates/foundation/peritus-codec"
owner = "B3"
layer = "foundation"
verification_class = "H"
[[packages]]
name = "peritus-collaboration"
path = "crates/orchestration/peritus-collaboration"
owner = "D3"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-context"
path = "crates/orchestration/peritus-context"
owner = "C6"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-daemon"
path = "crates/app/peritus-daemon"
owner = "G0"
layer = "app"
verification_class = "H"
[[packages]]
name = "peritus-debugger"
path = "crates/analysis/peritus-debugger"
owner = "E2"
layer = "analysis"
verification_class = "H"
[[packages]]
name = "peritus-eval"
path = "crates/analysis/peritus-eval"
owner = "E3"
layer = "analysis"
verification_class = "H"
[[packages]]
name = "peritus-evolution"
path = "crates/analysis/peritus-evolution"
owner = "F0"
layer = "analysis"
verification_class = "H"
[[packages]]
name = "peritus-evidence"
path = "crates/state/peritus-evidence"
owner = "C0"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-gates"
path = "crates/orchestration/peritus-gates"
owner = "D1"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-git"
path = "crates/runtime/peritus-git"
owner = "C1"
layer = "runtime"
verification_class = "H"
[[packages]]
name = "peritus-harness"
path = "crates/orchestration/peritus-harness"
owner = "E1"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-journal"
path = "crates/state/peritus-journal"
owner = "C0"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-kernel"
path = "crates/foundation/peritus-kernel"
owner = "B0"
layer = "foundation"
verification_class = "V"
[[packages]]
name = "peritus-leases"
path = "crates/state/peritus-leases"
owner = "B1"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-mcp"
path = "crates/extensions/peritus-mcp"
owner = "G3"
layer = "extensions"
verification_class = "H"
[[packages]]
name = "peritus-memory"
path = "crates/orchestration/peritus-memory"
owner = "C6"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-migrations"
path = "crates/state/peritus-migrations"
owner = "C0"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-model-protocol"
path = "crates/model/peritus-model-protocol"
owner = "C5"
layer = "model"
verification_class = "H"
[[packages]]
name = "peritus-network"
path = "crates/runtime/peritus-network"
owner = "C3"
layer = "runtime"
verification_class = "H"
[[packages]]
name = "peritus-orchestrator"
path = "crates/orchestration/peritus-orchestrator"
owner = "E0"
layer = "orchestration"
verification_class = "H"
[[packages]]
name = "peritus-run-knowledge"
path = "crates/orchestration/peritus-run-knowledge"
owner = "C6"
layer = "orchestration"
verification_class = "V"
[[packages]]
name = "peritus-run-settlement"
path = "crates/orchestration/peritus-run-settlement"
owner = "E0"
layer = "orchestration"
verification_class = "V"
[[packages]]
name = "peritus-patch"
path = "crates/runtime/peritus-patch"
owner = "C1"
layer = "runtime"
verification_class = "H"
[[packages]]
name = "peritus-plugin-host"
path = "crates/extensions/peritus-plugin-host"
owner = "G3"
layer = "extensions"
verification_class = "H"
[[packages]]
name = "peritus-plugin-sdk"
path = "crates/extensions/peritus-plugin-sdk"
owner = "G3"
layer = "extensions"
verification_class = "H"
[[packages]]
name = "peritus-policy"
path = "crates/foundation/peritus-policy"
owner = "B1"
layer = "foundation"
verification_class = "V"
[[packages]]
name = "peritus-projection"
path = "crates/state/peritus-projection"
owner = "C0"
layer = "state"
verification_class = "H"
[[packages]]
name = "peritus-protocol"
path = "crates/foundation/peritus-protocol"
owner = "B3"
layer = "foundation"
verification_class = "H"
[[packages]]
name = "peritus-provider-anthropic"
path = "crates/model/peritus-provider-anthropic"
owner = "C5"
layer = "model"
verification_class = "H"
[[packages]]
name = "peritus-provider-compatible"
path = "crates/model/peritus-provider-compatible"
owner = "C5"
layer = "model"
verification_class = "H"
[[packages]]
name = "peritus-provider-core"
path = "crates/model/peritus-provider-core"
owner = "C5"
layer = "model"
verification_class = "H"
[[packages]]
name = "peritus-provider-google"
path = "crates/model/peritus-provider-google"
owner = "C5"
layer = "model"
verification_class = "H"
[[packages]]
name = "peritus-provider-openai"
path = "crates/model/peritus-provider-openai"
owner = "C5"
layer = "model"
verification_class = "H"

[[packages]]
name = "peritus-process"
path = "crates/runtime/peritus-process"
owner = "C2"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-product-runner"
path = "crates/app/peritus-product-runner"
owner = "G4"
layer = "app"
verification_class = "H"

[[packages]]
name = "peritus-product-state"
path = "crates/app/peritus-product-state"
owner = "G4"
layer = "app"
verification_class = "H"

[[packages]]
name = "peritus-quality-policy"
path = "crates/foundation/peritus-quality-policy"
owner = "B2"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-release-policy"
path = "crates/foundation/peritus-release-policy"
owner = "H4"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-review"
path = "crates/orchestration/peritus-review"
owner = "D2"
layer = "orchestration"
verification_class = "H"

[[packages]]
name = "peritus-role"
path = "crates/orchestration/peritus-role"
owner = "C6"
layer = "orchestration"
verification_class = "V"

[[packages]]
name = "peritus-sandbox"
path = "crates/runtime/peritus-sandbox"
owner = "C2"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-sandbox-linux"
path = "crates/runtime/peritus-sandbox-linux"
owner = "C3"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-sandbox-macos"
path = "crates/runtime/peritus-sandbox-macos"
owner = "C3"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-sandbox-windows"
path = "crates/runtime/peritus-sandbox-windows"
owner = "C3"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-scheduler"
path = "crates/orchestration/peritus-scheduler"
owner = "D3"
layer = "orchestration"
verification_class = "H"

[[packages]]
name = "peritus-secrets"
path = "crates/runtime/peritus-secrets"
owner = "C3"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-security-policy"
path = "crates/foundation/peritus-security-policy"
owner = "H0"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-spec"
path = "crates/foundation/peritus-spec"
owner = "B2"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-telemetry"
path = "crates/observe/peritus-telemetry"
owner = "C7"
layer = "observe"
verification_class = "H"

[[packages]]
name = "peritus-tool-protocol"
path = "crates/tools/peritus-tool-protocol"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-tool-router"
path = "crates/tools/peritus-tool-router"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-tools-fs"
path = "crates/tools/peritus-tools-fs"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-tools-git"
path = "crates/tools/peritus-tools-git"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-tools-quality"
path = "crates/tools/peritus-tools-quality"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-tools-shell"
path = "crates/tools/peritus-tools-shell"
owner = "C4"
layer = "tools"
verification_class = "H"

[[packages]]
name = "peritus-trace"
path = "crates/observe/peritus-trace"
owner = "C7"
layer = "observe"
verification_class = "H"

[[packages]]
name = "peritus-types"
path = "crates/foundation/peritus-types"
owner = "A1"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-workspace"
path = "crates/runtime/peritus-workspace"
owner = "C1"
layer = "runtime"
verification_class = "H"

[[packages]]
name = "peritus-tcb"
path = "crates/foundation/peritus-tcb"
owner = "A1"
layer = "foundation"
verification_class = "T"
"#;
