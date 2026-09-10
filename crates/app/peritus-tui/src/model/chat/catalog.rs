//! Typed local command metadata. Only directly entered commands are executable intents.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Arguments {
    None,
    Message,
    Model,
    Effort,
    Sessions,
    Fork,
    Queue,
    Context,
    Compact,
    Brief,
    Preview,
    Checkpoint,
    Rewind,
    Attachment,
    Goal,
    Pause,
    Resume,
    Budget,
    Permissions,
    Init,
    Memory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Command {
    Chat,
    Plan,
    Review,
    Build,
    Model,
    Effort,
    New,
    Status,
    Diff,
    Runs,
    Trace,
    Terminal,
    Approvals,
    Details,
    Stop,
    Accept,
    Commit,
    Export,
    Discard,
    Run,
    Reconnect,
    Doctor,
    Sessions,
    Fork,
    Queue,
    Context,
    Compact,
    Brief,
    Goal,
    Pause,
    Resume,
    Usage,
    Budget,
    Preview,
    Checkpoint,
    Rewind,
    Attach,
    Files,
    Permissions,
    Init,
    Memory,
    Help,
    Quit,
}

mod entries;
use entries::COMMANDS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommandSpec {
    pub(super) command: Command,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    arguments: Arguments,
}

pub(super) fn parse(text: &str) -> Result<(Command, &str), &'static str> {
    let (name, arguments) = text
        .split_once(char::is_whitespace)
        .map_or((text, ""), |(name, arguments)| (name, arguments.trim()));
    let spec = COMMANDS
        .iter()
        .find(|spec| spec.name == name)
        .ok_or("Unknown slash command. Type / to see available commands.")?;
    if spec.arguments == Arguments::None && !arguments.is_empty() {
        return Err("This command takes no arguments; draft retained.");
    }
    Ok((spec.command, arguments))
}

/// Complete command names without invoking a provider; exact matches come first.
pub(super) fn completions(text: &str) -> Vec<(String, &'static str)> {
    if !text.starts_with('/') {
        return Vec::new();
    }
    if text.contains(char::is_whitespace) {
        return argument_completions(text);
    }
    let mut matches: Vec<_> = COMMANDS
        .iter()
        .filter(|spec| spec.name.starts_with(text))
        .map(|spec| (spec.name.to_owned(), spec.description))
        .collect();
    matches.sort_by_key(|(name, _)| name != text);
    matches
}

fn argument_completions(text: &str) -> Vec<(String, &'static str)> {
    let Some((prefix, partial)) = text.rsplit_once(char::is_whitespace) else { return Vec::new() };
    let words: Vec<_> = prefix.split_whitespace().collect();
    let choices: &[&str] = match words.as_slice() {
        ["/brief"] => &["objective", "acceptance", "constraints", "assumptions"],
        ["/goal"] => &["confirm", "clear", "criterion"],
        ["/goal", "criterion"] => &["graphical", "remove-graphical"],
        ["/goal", "clear"] => &["confirm"],
        ["/pause"] => &["now", "after-operation", "before-edit"],
        ["/budget"] => &["none", "time=", "requests=", "tools=", "tokens="],
        ["/preview"] => &["results", "launch", "play", "capture", "stop", "check", "feedback"],
        ["/context"] => &["next", "history", "show", "more", "previous"],
        ["/queue"] => &[
            "add", "edit", "correct", "hold", "release", "withdraw", "order", "history", "pending",
            "show", "next", "previous", "retry",
        ],
        ["/sessions"] => {
            &["new", "open", "rename", "pin", "unpin", "archive", "unarchive", "retry"]
        }
        ["/fork"] => &["read-only", "isolated"],
        ["/checkpoint"] => &["show"],
        ["/permissions"] => &["restrict", "grant"],
        ["/permissions", "restrict" | "grant"] => &["read", "write", "process", "network"],
        ["/init"] => &["apply", "decline"],
        ["/memory"] => &[
            "save",
            "revise",
            "pin",
            "unpin",
            "project",
            "conversation",
            "forget",
            "history",
            "more",
            "previous",
        ],
        ["/effort"] => &[
            "writer", "reviewer", "fixer", "default", "minimal", "low", "medium", "high", "xhigh",
            "max", "ultra",
        ],
        ["/effort", "writer" | "reviewer" | "fixer"] => {
            &["default", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
        }
        ["/model"] => &["writer", "reviewer", "fixer", "refresh", "manual"],
        ["/model", "writer" | "reviewer" | "fixer"] => &["refresh", "manual"],
        _ => &[],
    };
    let mut matches: Vec<_> = choices
        .iter()
        .filter(|choice| choice.starts_with(partial))
        .map(|choice| (format!("{prefix} {choice}"), "Tab completes · Enter submits"))
        .collect();
    matches.sort_by_key(|(candidate, _)| candidate != text);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_unique_and_every_entry_dispatches_and_completes() {
        for (index, spec) in COMMANDS.iter().enumerate() {
            assert!(!COMMANDS[..index].iter().any(|prior| prior.name == spec.name));
            assert_eq!(parse(spec.name), Ok((spec.command, "")));
            assert_eq!(
                completions(spec.name).first(),
                Some(&(spec.name.to_owned(), spec.description))
            );
            if spec.arguments == Arguments::None {
                assert!(parse(&format!("{} unexpected", spec.name)).is_err());
            }
        }
        assert!(parse("/unknown").is_err());
        assert!(parse("/STOP").is_err());
        assert!(parse(" /stop").is_err());
        assert!(completions("a /stop").is_empty());
    }

    #[test]
    fn completion_is_argument_aware_and_never_rewrites_freeform_content() {
        for (prefix, expected) in [
            ("/effort reviewer x", "/effort reviewer xhigh"),
            ("/sessions ren", "/sessions rename"),
            ("/model writer man", "/model writer manual"),
        ] {
            assert_eq!(completions(prefix).first().expect("completion").0, expected);
        }
        for text in [
            "/sessions new exact title",
            "/model writer manual arbitrary-id",
            "/chat keep this message",
            "/doctor repair",
        ] {
            assert!(completions(text).is_empty(), "{text}");
        }
    }
}
