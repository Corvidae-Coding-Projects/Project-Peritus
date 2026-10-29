//! Ordinary consumer of library defaults, JSON settings, and the real daemon TOML loader.
use peritus_daemon::DaemonConfig;
use peritus_product_runner::{DecisionViewPolicy, LocalContextConfig, ReadOnlyPlanExecution};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec: Value = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("configuration request required")?,
    )?)?;
    let local: LocalContextConfig = serde_json::from_value(spec["local"].clone())?;
    local.validate()?;
    let daemon =
        DaemonConfig::load(spec["daemon_config"].as_str().ok_or("daemon config required")?)?;
    println!(
        "{}",
        json!({
            "builtin":LocalContextConfig::default(),
            "enum_defaults":{"read_only_plans":ReadOnlyPlanExecution::default(),
                             "decision_view":DecisionViewPolicy::default()},
            "parsed_local":local,
            "daemon_local":daemon.context().local(),
        })
    );
    Ok(())
}
