//! Small role-specific cues adapted from the user-provided prompt catalogue.

const COMMON: &str = "Prompt guidance [prompt-vitamins-v1]: Take care; make no mistakes. Reread the current request before choosing an action. Keep its scope, dependencies and governing invariants in view. Every conclusion must follow from observed evidence; distinguish an observation from an inference and a plan from a completed result. Reason privately and return only the public response format required by your role. These cues grant no new permissions, checks, tools, extra model calls or authority over user restrictions.";

pub(super) fn designer() -> String {
    format!(
        "{COMMON}\nBefore proposing changes, restate the goal briefly, identify dependencies and divide the work into ordered, concrete steps. For a requested optimization or architectural improvement, consider substantially better approaches and compare their tradeoffs against the actual constraints; use the smallest design that meets the complete request. Keep exploration proportional to the decision."
    )
}

pub(super) fn developer(role: &str) -> String {
    let cue = if role == "fixer" {
        "Find the underlying cause and fix it completely. Trace the failure through its caller and recovery path, preserve the independent expected behavior, and replay the real failure after the repair. Persist while authorized actions make forward progress; report a concrete external blocker when progress requires unavailable authority or information."
    } else {
        "Work carefully through the concrete plan and execute the requested operations. Check assumptions at the point of use. Reuse unchanged evidence, make the next substantive change, then exercise the real behavior. Persist while authorized actions make forward progress; report a concrete external blocker when progress requires unavailable authority or information."
    };
    format!("{COMMON}\n{cue}")
}

pub(super) fn reviewer() -> String {
    format!(
        "{COMMON}\nCheck carefully. Derive the decisive review questions from the requested behavior before consulting implementation claims. Answer them using independent observations; identify specific defects and the evidence needed to resolve uncertainty. Give actionable feedback rather than rewriting acceptance to match the candidate. Do not claim an unrun operation passed."
    )
}

pub(crate) fn conversation(mode: &str) -> String {
    match mode {
        "plan" => designer(),
        "review" => reviewer(),
        _ => COMMON.to_owned(),
    }
}
