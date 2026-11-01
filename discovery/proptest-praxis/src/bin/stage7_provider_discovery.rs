//! Public-interface probe driven by a deterministic, isolated PATH/HOME schedule.

use peritus_product_state::ProviderKind;
use peritus_provider_anthropic::ClaudeExecutable;
use peritus_provider_onboarding::AccountProvider;
use peritus_provider_openai::CodexExecutable;
use serde_json::json;

fn main() {
    let kind = match std::env::args().nth(1).as_deref() {
        Some("codex") => ProviderKind::CodexAccount,
        Some("claude") => ProviderKind::ClaudeAccount,
        _ => {
            eprintln!("expected codex or claude");
            std::process::exit(2);
        }
    };
    let raw = match kind {
        ProviderKind::CodexAccount => {
            CodexExecutable::discover().map(|value| value.as_path().to_owned())
        }
        ProviderKind::ClaudeAccount => {
            ClaudeExecutable::discover().map(|value| value.as_path().to_owned())
        }
        _ => unreachable!(),
    };
    let onboarded = AccountProvider::discover(kind).map(|value| value.executable().to_owned());
    println!(
        "{}",
        json!({
            "raw":raw.as_ref().ok().map(|path|path.to_string_lossy().into_owned()),
            "onboarding":onboarded.as_ref().ok().map(|path|path.to_string_lossy().into_owned()),
            "raw_error":raw.err().map(|error|error.to_string()),
            "onboarding_error":onboarded.err().map(|error|error.to_string()),
        })
    );
}
