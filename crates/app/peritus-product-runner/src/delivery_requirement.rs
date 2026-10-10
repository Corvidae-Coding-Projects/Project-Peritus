//! Deterministic distinction between supporting files and requested live effects.

use crate::execution::ProductDeliveryScope;

/// Whether the original task requires a live external result in addition to any workspace files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalEffectRequirement {
    /// External effects are authorized, but workspace artifacts may be the complete result.
    Optional,
    /// The request itself demands a live operational result.
    Required,
}

impl ExternalEffectRequirement {
    /// Classifies only explicit operational imperatives under an already-authorized effect scope.
    #[must_use]
    pub fn from_task(scope: ProductDeliveryScope, task: &str) -> Self {
        if !scope.allows_external_effects() {
            return Self::Optional;
        }
        let normalized = task.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
        let request = request_clause(&normalized);
        let mut words = request
            .split_whitespace()
            .map(|word| word.trim_matches(|character: char| !character.is_ascii_alphanumeric()))
            .filter(|word| !word.is_empty());
        let first = words.next().unwrap_or_default();
        let second = words.next().unwrap_or_default();
        let intrinsic = matches!(
            first,
            "deploy" | "disable" | "enable" | "install" | "restart" | "serve" | "uninstall"
        );
        let operational = intrinsic
            || matches!(first, "configure" | "host" | "launch" | "run" | "start" | "stop")
            || (first == "set" && second == "up");
        let live_outcome = [
            "accessible at",
            "available at",
            "block until",
            "connect to",
            "curl http",
            "in the background",
            "leave it running",
            "listen on",
            "listening on",
            "login prompt",
            "running on port",
            "so i can",
            "so that i can",
            "when i run",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase));
        let executable_outcome =
            ["running this", "should run", "so i can run", "so that i can run", "when i run"]
                .iter()
                .any(|phrase| normalized.contains(phrase));
        let executable_request =
            matches!(first, "build" | "develop" | "implement") && executable_outcome;
        let install_sequence = matches!(first, "build" | "compile" | "download" | "fetch")
            && [" and install ", " then install ", "compile and install"]
                .iter()
                .any(|phrase| normalized.contains(phrase));
        if intrinsic || (operational && live_outcome) || executable_request || install_sequence {
            Self::Required
        } else {
            Self::Optional
        }
    }

    /// Whether helper files alone are insufficient acceptance evidence.
    #[must_use]
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

fn request_clause(task: &str) -> &str {
    for marker in [". please ", "! please ", "? please ", "; please ", ", please "] {
        if let Some((_, request)) = task.split_once(marker) {
            return request;
        }
    }
    [
        "please ",
        "can you please ",
        "could you please ",
        "would you please ",
        "can you ",
        "could you ",
        "would you ",
        "i need you to ",
        "i want you to ",
    ]
    .iter()
    .find_map(|prefix| task.strip_prefix(prefix))
    .unwrap_or(task)
}
