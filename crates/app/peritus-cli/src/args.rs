use std::{collections::VecDeque, ffi::OsString, path::PathBuf, time::Duration};

use peritus_types::SessionId;

use crate::{completion::Shell, error::CliError, id::parse_hex_id};

mod artifact;
mod product;
mod terminal;
mod types;

#[cfg(test)]
mod tests;

pub use types::*;

impl Cli {
    pub(crate) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        let mut parser = Parser::new(arguments);
        let _executable = parser.pop();
        let mut endpoint = None;
        let mut session = None;
        let mut timeout = Duration::from_secs(30);
        let mut json = false;
        loop {
            match parser.peek_utf8()? {
                Some("--endpoint") => {
                    parser.pop();
                    set_once(&mut endpoint, parser.value_os("--endpoint")?, "--endpoint")?;
                }
                Some("--session") => {
                    parser.pop();
                    let value = parser.value_utf8("--session")?;
                    let bytes = parse_hex_id(&value, "--session")?;
                    let parsed = SessionId::new(bytes)
                        .map_err(|_| CliError::usage("invalid --session identifier"))?;
                    set_once(&mut session, parsed, "--session")?;
                }
                Some("--timeout-seconds") => {
                    parser.pop();
                    let value = positive_u64(
                        &parser.value_utf8("--timeout-seconds")?,
                        "--timeout-seconds",
                    )?;
                    timeout = Duration::from_secs(value);
                }
                Some("--json") => {
                    parser.pop();
                    if json {
                        return Err(CliError::usage("--json may be supplied only once"));
                    }
                    json = true;
                }
                Some("-h" | "--help") => {
                    parser.pop();
                    parser.finish()?;
                    return Ok(Self {
                        endpoint,
                        session,
                        timeout,
                        json,
                        command: Command::Help { text: HELP.to_owned() },
                    });
                }
                Some("-V" | "--version") => {
                    parser.pop();
                    parser.finish()?;
                    return Ok(Self {
                        endpoint,
                        session,
                        timeout,
                        json,
                        command: Command::Version,
                    });
                }
                Some(value) if value.starts_with('-') => {
                    return Err(CliError::usage(format!("unknown global option: {value}")));
                }
                _ => break,
            }
        }
        let command = parse_command(&mut parser)?;
        Ok(Self { endpoint, session, timeout, json, command })
    }
}

fn parse_command(parser: &mut Parser) -> Result<Command, CliError> {
    let command = parser.command("command")?;
    match command.as_str() {
        "update" => parse_update(parser),
        "providers" => {
            parser.finish()?;
            Ok(Command::Providers)
        }
        "workspaces" => {
            parser.finish()?;
            Ok(Command::Workspaces)
        }
        "open" => {
            let path = parser.pop().map(PathBuf::from);
            parser.finish()?;
            Ok(Command::Open { path })
        }
        "status" => {
            parser.finish()?;
            Ok(Command::Status)
        }
        "shutdown" => parse_shutdown(parser),
        "command" => parse_command_family(parser),
        "events" => parse_events(parser),
        "artifact" => artifact::parse_artifact(parser),
        "prompt" => parse_prompt(parser),
        "terminal" => terminal::parse_terminal(parser),
        "runs" => product::parse_product(parser),
        "completions" => parse_completions(parser),
        "help" => {
            parser.finish()?;
            Ok(Command::Help { text: HELP.to_owned() })
        }
        _ => Err(CliError::usage(format!("unknown command: {command}"))),
    }
}

fn parse_update(parser: &mut Parser) -> Result<Command, CliError> {
    let automatic_checks = match parser.peek_utf8()? {
        Some("--enable-checks") => {
            parser.pop();
            Some(true)
        }
        Some("--disable-checks") => {
            parser.pop();
            Some(false)
        }
        Some(option) => return Err(CliError::usage(format!("unknown update option: {option}"))),
        None => None,
    };
    parser.finish()?;
    Ok(Command::Update(UpdateArgs { automatic_checks }))
}

fn parse_shutdown(parser: &mut Parser) -> Result<Command, CliError> {
    let wait = parser.take_flag("--wait")?;
    parser.finish()?;
    Ok(Command::Shutdown { wait })
}

fn parse_command_family(parser: &mut Parser) -> Result<Command, CliError> {
    require_subcommand(parser, "submit", "command")?;
    let mut actor = None;
    let mut envelope = None;
    let mut payload = None;
    let mut key = None;
    let mut bind_expected_revision = true;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--actor" => {
                parser.pop();
                let value = parser.value_utf8("--actor")?;
                set_once(&mut actor, parse_hex_id(&value, "--actor")?, "--actor")?;
            }
            "--envelope" => {
                parser.pop();
                set_once(&mut envelope, parser.value_path("--envelope")?, "--envelope")?;
            }
            "--payload" => {
                parser.pop();
                set_once(&mut payload, parser.value_path("--payload")?, "--payload")?;
            }
            "--idempotency-key" => {
                parser.pop();
                let value = parser.value_utf8("--idempotency-key")?.into_bytes();
                set_once(&mut key, value, "--idempotency-key")?;
            }
            "--no-expected-revision" => {
                parser.pop();
                if !bind_expected_revision {
                    return Err(CliError::usage("--no-expected-revision may be supplied once"));
                }
                bind_expected_revision = false;
            }
            _ => return Err(CliError::usage(format!("unknown command submit option: {option}"))),
        }
    }
    Ok(Command::Submit(SubmitArgs {
        actor: required(actor, "--actor")?,
        envelope: required(envelope, "--envelope")?,
        payload: required(payload, "--payload")?,
        idempotency_key: required(key, "--idempotency-key")?,
        bind_expected_revision,
    }))
}

fn parse_events(parser: &mut Parser) -> Result<Command, CliError> {
    require_subcommand(parser, "watch", "events")?;
    let mut topics = Vec::new();
    let mut after = 0;
    let mut window = 64;
    let mut count = None;
    let mut snapshot_acceptable = false;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--topic" => {
                parser.pop();
                topics.push(parser.value_utf8("--topic")?);
            }
            "--after" => {
                parser.pop();
                after = nonnegative_u64(&parser.value_utf8("--after")?, "--after")?;
            }
            "--window" => {
                parser.pop();
                window = positive_u32(&parser.value_utf8("--window")?, "--window")?;
            }
            "--count" => {
                parser.pop();
                count = Some(positive_u64(&parser.value_utf8("--count")?, "--count")?);
            }
            "--snapshot-acceptable" => {
                parser.pop();
                if snapshot_acceptable {
                    return Err(CliError::usage("--snapshot-acceptable may be supplied once"));
                }
                snapshot_acceptable = true;
            }
            _ => return Err(CliError::usage(format!("unknown events watch option: {option}"))),
        }
    }
    if topics.is_empty() {
        return Err(CliError::usage("events watch requires at least one --topic"));
    }
    topics.sort();
    if topics.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CliError::usage("--topic values must be unique"));
    }
    Ok(Command::Events(EventArgs { topics, after, window, count, snapshot_acceptable }))
}

fn parse_prompt(parser: &mut Parser) -> Result<Command, CliError> {
    match parser.command("prompt subcommand")?.as_str() {
        "answer" => parse_prompt_answer(parser),
        "cancel" => parse_prompt_cancel(parser),
        value => Err(CliError::usage(format!("unknown prompt subcommand: {value}"))),
    }
}

fn parse_prompt_answer(parser: &mut Parser) -> Result<Command, CliError> {
    let mut binding = None;
    let mut value = None;
    let mut rationale = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--binding" => {
                parser.pop();
                set_once(&mut binding, parser.value_path("--binding")?, "--binding")?;
            }
            "--signed-decision" => {
                parser.pop();
                let path = parser.value_path("--signed-decision")?;
                set_once(&mut value, PromptValue::SignedDecision(path), "prompt answer value")?;
            }
            "--text" => {
                parser.pop();
                let text = parser.value_utf8("--text")?;
                set_once(&mut value, PromptValue::Text(text), "prompt answer value")?;
            }
            "--selection" => {
                parser.pop();
                let selected = parser.value_utf8("--selection")?;
                set_once(&mut value, PromptValue::Selection(selected), "prompt answer value")?;
            }
            "--confirm" => {
                parser.pop();
                let confirmed = parse_bool(&parser.value_utf8("--confirm")?, "--confirm")?;
                set_once(&mut value, PromptValue::Confirmation(confirmed), "prompt answer value")?;
            }
            "--secret-reference" => {
                parser.pop();
                let reference = parser.value_utf8("--secret-reference")?;
                set_once(
                    &mut value,
                    PromptValue::SecretReference(reference),
                    "prompt answer value",
                )?;
            }
            "--rationale" => {
                parser.pop();
                set_once(&mut rationale, parser.value_utf8("--rationale")?, "--rationale")?;
            }
            _ => return Err(CliError::usage(format!("unknown prompt answer option: {option}"))),
        }
    }
    Ok(Command::PromptAnswer(PromptAnswerArgs {
        binding: required(binding, "--binding")?,
        value: required(value, "one prompt answer value")?,
        rationale,
    }))
}

fn parse_prompt_cancel(parser: &mut Parser) -> Result<Command, CliError> {
    let mut binding = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--binding" => {
                parser.pop();
                set_once(&mut binding, parser.value_path("--binding")?, "--binding")?;
            }
            _ => return Err(CliError::usage(format!("unknown prompt cancel option: {option}"))),
        }
    }
    Ok(Command::PromptCancel(PromptCancelArgs { binding: required(binding, "--binding")? }))
}

fn parse_completions(parser: &mut Parser) -> Result<Command, CliError> {
    let value = parser.command("completion shell")?;
    parser.finish()?;
    let shell = Shell::parse(&value).ok_or_else(|| {
        CliError::usage("completion shell must be bash, zsh, fish, or powershell")
    })?;
    Ok(Command::Completions(shell))
}

fn require_subcommand(parser: &mut Parser, expected: &str, family: &str) -> Result<(), CliError> {
    let actual = parser.command(&format!("{family} subcommand"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(CliError::usage(format!("unknown {family} subcommand: {actual}")))
    }
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, CliError> {
    value.ok_or_else(|| CliError::usage(format!("missing required {name}")))
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), CliError> {
    if slot.replace(value).is_some() {
        Err(CliError::usage(format!("{name} may be supplied only once")))
    } else {
        Ok(())
    }
}

fn positive_u64(value: &str, name: &str) -> Result<u64, CliError> {
    let parsed = nonnegative_u64(value, name)?;
    if parsed == 0 { Err(CliError::usage(format!("{name} must be positive"))) } else { Ok(parsed) }
}

fn nonnegative_u64(value: &str, name: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::usage(format!("{name} must be an unsigned decimal integer")))
}

fn positive_u32(value: &str, name: &str) -> Result<u32, CliError> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| CliError::usage(format!("{name} must be a positive 32-bit integer")))?;
    if parsed == 0 { Err(CliError::usage(format!("{name} must be positive"))) } else { Ok(parsed) }
}

fn positive_u16(value: &str, name: &str) -> Result<u16, CliError> {
    let parsed = value
        .parse::<u16>()
        .map_err(|_| CliError::usage(format!("{name} must be a positive 16-bit integer")))?;
    if parsed == 0 { Err(CliError::usage(format!("{name} must be positive"))) } else { Ok(parsed) }
}

fn parse_bool(value: &str, name: &str) -> Result<bool, CliError> {
    match value {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(CliError::usage(format!("{name} must be true or false"))),
    }
}

struct Parser {
    arguments: VecDeque<OsString>,
}

impl Parser {
    fn new(arguments: impl IntoIterator<Item = OsString>) -> Self {
        Self { arguments: arguments.into_iter().collect() }
    }

    fn pop(&mut self) -> Option<OsString> {
        self.arguments.pop_front()
    }

    fn peek_utf8(&self) -> Result<Option<&str>, CliError> {
        self.arguments
            .front()
            .map(|value| {
                value.to_str().ok_or_else(|| CliError::usage("option or command is not UTF-8"))
            })
            .transpose()
    }

    fn command(&mut self, name: &str) -> Result<String, CliError> {
        let value =
            self.pop().ok_or_else(|| CliError::usage(format!("missing {name}; see --help")))?;
        value.into_string().map_err(|_| CliError::usage(format!("{name} is not UTF-8")))
    }

    fn value_os(&mut self, option: &str) -> Result<OsString, CliError> {
        self.pop().ok_or_else(|| CliError::usage(format!("{option} requires a value")))
    }

    fn value_utf8(&mut self, option: &str) -> Result<String, CliError> {
        self.value_os(option)?
            .into_string()
            .map_err(|_| CliError::usage(format!("{option} value is not UTF-8")))
    }

    fn value_path(&mut self, option: &str) -> Result<PathBuf, CliError> {
        Ok(PathBuf::from(self.value_os(option)?))
    }

    fn take_flag(&mut self, flag: &str) -> Result<bool, CliError> {
        match self.peek_utf8()? {
            Some(value) if value == flag => {
                self.pop();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn finish(&self) -> Result<(), CliError> {
        self.arguments.front().map_or_else(
            || Ok(()),
            |argument| {
                Err(CliError::usage(format!("unexpected argument: {}", argument.to_string_lossy())))
            },
        )
    }
}
