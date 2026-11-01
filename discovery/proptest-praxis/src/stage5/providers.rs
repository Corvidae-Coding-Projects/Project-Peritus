//! All public provider boundary modes; first-party OpenAI HTTP is a private owner target.

#[path = "providers/common.rs"]
mod common;
#[path = "providers/http.rs"]
mod http;
#[path = "providers/runtime.rs"]
mod runtime;

pub use common::{Case, Mode};
use proptest::strategy::BoxedStrategy;

const MODES: &[Mode] = &[
    Mode::Anthropic,
    Mode::GoogleGenerate,
    Mode::GoogleInteractions,
    Mode::CompatibleChat,
    Mode::CompatibleResponses,
    Mode::Codex,
    Mode::Claude,
    Mode::OpenCode,
];

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    common::strategy(MODES, max_steps)
}
pub fn examples() -> Vec<Case> {
    let mut cases = common::examples(MODES);
    cases.push(Case { legacy: true, ..common::example(Mode::OpenCode) });
    cases.push(Case { legacy: true, variant: true, ..common::example(Mode::OpenCode) });
    cases
}
pub fn calibration() -> Case {
    common::example(Mode::GoogleInteractions)
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if !MODES.contains(&case.mode) {
        return Err("provider mode belongs to separate private owner campaign".into());
    }
    if case.payload.len() > 256
        || case.chunks.len() > 64
        || case.input < 8
        || case.output < 4
        || case.effort > 3
    {
        return Err("provider case exceeds fixture bounds".into());
    }
    let mut metrics = if let Some(control) = common::positive_control(case) {
        let mut metrics = check(&control)?;
        common::bump(&mut metrics, "paired_valid_transport_controls");
        metrics
    } else {
        crate::Metrics::new()
    };
    let result = if case.mode.runtime() { runtime::check(case)? } else { http::check(case)? };
    for (name, count) in result {
        *metrics.entry(name).or_default() += count;
    }
    Ok(metrics)
}
