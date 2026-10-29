//! A promise in public prose is not a host-owned pipeline handoff receipt.

pub(super) const POLICY: &str = "\n\nA final conversational reply ends this invocation; no work runs after it. For authorized work, call run_pipeline before ending the turn. Saying you will run, launch, or hand work to the pipeline does not schedule it. If you cannot hand off, explain the actual blocker or ask the necessary question. If the user requested only discussion, answer it without promising execution.";

pub(super) const CORRECTION: &str = "your reply promises pipeline execution, but this invocation has no successful run_pipeline receipt. Nothing has been scheduled. For understood work authorized by the current user request, call run_pipeline now. Otherwise answer the read-only request, ask the necessary clarification, or state the actual blocker without claiming that execution is starting. This correction grants no new authority";

/// Detects direct first-person execution promises, excluding quoted blocks, code, and
/// conditional offers. This only rejects a misleading reply; it never launches any work.
pub(super) fn promises_execution(reply: &str) -> bool {
    let mut fenced = false;
    let mut prose = String::new();
    for line in reply.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            prose.push('\n');
            continue;
        }
        if fenced || line.starts_with('>') {
            prose.push('\n');
            continue;
        }
        if line.is_empty() {
            prose.push('\n');
        } else {
            prose.push_str(line);
            prose.push(' ');
        }
    }
    prose
        .to_lowercase()
        .replace(['’', '‘'], "'")
        .split(['.', '!', '?', '\n'])
        .any(promised_sentence)
}

fn promised_sentence(sentence: &str) -> bool {
    if !sentence.contains("pipeline")
        || ["if you", "when you", "once you", "after you"].iter().any(|s| sentence.contains(s))
    {
        return false;
    }
    ["i'll ", "i will ", "i'm going to ", "i am going to ", "i'm ", "i am "].into_iter().any(
        |marker| {
            sentence.match_indices(marker).any(|(index, _)| {
                let before = &sentence[..index];
                if before
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || ['\'', '"', '`', '“'].contains(&c))
                {
                    return false;
                }
                let mut action = &sentence[index + marker.len()..];
                while let Some(rest) = ["now ", "then ", "first ", "also ", "just ", "immediately "]
                    .into_iter()
                    .find_map(|prefix| action.strip_prefix(prefix))
                {
                    action = rest;
                }
                matches!(
                    action.split_whitespace().next(),
                    Some(
                        "have"
                            | "having"
                            | "ask"
                            | "asking"
                            | "run"
                            | "running"
                            | "start"
                            | "starting"
                            | "launch"
                            | "launching"
                            | "hand"
                            | "handing"
                            | "send"
                            | "sending"
                            | "invoke"
                            | "invoking"
                            | "delegate"
                            | "delegating"
                            | "queue"
                            | "queueing"
                            | "use"
                            | "using"
                            | "call"
                            | "calling"
                            | "let"
                            | "letting"
                            | "get"
                            | "getting"
                            | "route"
                            | "routing"
                            | "pass"
                            | "passing"
                            | "schedule"
                            | "scheduling"
                            | "kick"
                            | "kicking"
                    )
                )
            })
        },
    )
}
