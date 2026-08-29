use std::{collections::VecDeque, ffi::OsString, path::PathBuf, time::Duration};

use peritus_types::SessionId;

use crate::{completion::Shell, error::CliError, id::parse_hex_id};

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
        "update" => {
            parser.finish()?;
            Ok(Command::Update)
        }
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
        "artifact" => parse_artifact(parser),
        "prompt" => parse_prompt(parser),
        "terminal" => parse_terminal(parser),
        "completions" => parse_completions(parser),
        "help" => {
            parser.finish()?;
            Ok(Command::Help { text: HELP.to_owned() })
        }
        _ => Err(CliError::usage(format!("unknown command: {command}"))),
    }
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

fn parse_artifact(parser: &mut Parser) -> Result<Command, CliError> {
    match parser.command("artifact subcommand")?.as_str() {
        "get" => parse_artifact_get(parser),
        "put" => parse_artifact_put(parser),
        "cancel" => parse_artifact_cancel(parser),
        value => Err(CliError::usage(format!("unknown artifact subcommand: {value}"))),
    }
}

fn parse_artifact_cancel(parser: &mut Parser) -> Result<Command, CliError> {
    let mut transfer = None;
    let mut artifact = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--transfer" => {
                parser.pop();
                let value = parser.value_utf8("--transfer")?;
                set_once(&mut transfer, parse_hex_id(&value, "--transfer")?, "--transfer")?;
            }
            "--artifact" => {
                parser.pop();
                let value = parser.value_utf8("--artifact")?;
                set_once(&mut artifact, parse_hex_id(&value, "--artifact")?, "--artifact")?;
            }
            _ => return Err(CliError::usage(format!("unknown artifact cancel option: {option}"))),
        }
    }
    Ok(Command::ArtifactCancel(ArtifactCancelArgs {
        transfer: required(transfer, "--transfer")?,
        artifact: required(artifact, "--artifact")?,
    }))
}

fn parse_artifact_get(parser: &mut Parser) -> Result<Command, CliError> {
    let mut artifact = None;
    let mut output = None;
    let mut force = false;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--artifact" => {
                parser.pop();
                let value = parser.value_utf8("--artifact")?;
                set_once(&mut artifact, parse_hex_id(&value, "--artifact")?, "--artifact")?;
            }
            "--output" => {
                parser.pop();
                set_once(&mut output, parser.value_path("--output")?, "--output")?;
            }
            "--force" => {
                parser.pop();
                if force {
                    return Err(CliError::usage("--force may be supplied once"));
                }
                force = true;
            }
            _ => return Err(CliError::usage(format!("unknown artifact get option: {option}"))),
        }
    }
    Ok(Command::ArtifactGet(ArtifactGetArgs {
        artifact: required(artifact, "--artifact")?,
        output: required(output, "--output")?,
        force,
    }))
}

fn parse_artifact_put(parser: &mut Parser) -> Result<Command, CliError> {
    let mut artifact = None;
    let mut input = None;
    let mut media_type = None;
    let mut chunk_size = 64 * 1024;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--artifact" => {
                parser.pop();
                let value = parser.value_utf8("--artifact")?;
                set_once(&mut artifact, parse_hex_id(&value, "--artifact")?, "--artifact")?;
            }
            "--input" => {
                parser.pop();
                set_once(&mut input, parser.value_path("--input")?, "--input")?;
            }
            "--media-type" => {
                parser.pop();
                set_once(&mut media_type, parser.value_utf8("--media-type")?, "--media-type")?;
            }
            "--chunk-size" => {
                parser.pop();
                chunk_size = positive_u32(&parser.value_utf8("--chunk-size")?, "--chunk-size")?;
            }
            _ => return Err(CliError::usage(format!("unknown artifact put option: {option}"))),
        }
    }
    Ok(Command::ArtifactPut(ArtifactPutArgs {
        artifact: required(artifact, "--artifact")?,
        input: required(input, "--input")?,
        media_type: required(media_type, "--media-type")?,
        chunk_size,
    }))
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

fn parse_terminal(parser: &mut Parser) -> Result<Command, CliError> {
    match parser.command("terminal subcommand")?.as_str() {
        "attach" => parse_terminal_attach(parser),
        "input" => parse_terminal_input(parser),
        "resize" => parse_terminal_resize(parser),
        "detach" => Ok(Command::TerminalDetach(parse_terminal_binding(parser)?)),
        "cancel" => Ok(Command::TerminalCancel(parse_terminal_binding(parser)?)),
        value => Err(CliError::usage(format!("unknown terminal subcommand: {value}"))),
    }
}

fn parse_terminal_attach(parser: &mut Parser) -> Result<Command, CliError> {
    let mut process = None;
    let mut follow = true;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--process" => {
                parser.pop();
                let value = parser.value_utf8("--process")?;
                set_once(&mut process, parse_hex_id(&value, "--process")?, "--process")?;
            }
            "--no-follow" => {
                parser.pop();
                if !follow {
                    return Err(CliError::usage("--no-follow may be supplied once"));
                }
                follow = false;
            }
            _ => return Err(CliError::usage(format!("unknown terminal attach option: {option}"))),
        }
    }
    Ok(Command::TerminalAttach(TerminalAttachArgs {
        process: required(process, "--process")?,
        follow,
    }))
}

fn parse_terminal_input(parser: &mut Parser) -> Result<Command, CliError> {
    let (binding, input) = parse_terminal_binding_with(parser, true, false)?;
    Ok(Command::TerminalInput(TerminalInputArgs {
        binding,
        input: input.ok_or_else(|| CliError::usage("missing required --input"))?,
    }))
}

fn parse_terminal_resize(parser: &mut Parser) -> Result<Command, CliError> {
    let mut columns = None;
    let mut rows = None;
    let (binding, ()) = parse_terminal_binding_extra(parser, |option, parser| match option {
        "--columns" => {
            parser.pop();
            set_once(
                &mut columns,
                positive_u16(&parser.value_utf8("--columns")?, "--columns")?,
                "--columns",
            )?;
            Ok(true)
        }
        "--rows" => {
            parser.pop();
            set_once(&mut rows, positive_u16(&parser.value_utf8("--rows")?, "--rows")?, "--rows")?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(Command::TerminalResize(TerminalResizeArgs {
        binding,
        columns: required(columns, "--columns")?,
        rows: required(rows, "--rows")?,
    }))
}

fn parse_terminal_binding(parser: &mut Parser) -> Result<TerminalBindingArgs, CliError> {
    parse_terminal_binding_with(parser, false, false).map(|(binding, _)| binding)
}

fn parse_terminal_binding_with(
    parser: &mut Parser,
    allow_input: bool,
    _unused: bool,
) -> Result<(TerminalBindingArgs, Option<OsString>), CliError> {
    let mut input = None;
    let (binding, ()) = parse_terminal_binding_extra(parser, |option, parser| {
        if allow_input && option == "--input" {
            parser.pop();
            set_once(&mut input, parser.value_os("--input")?, "--input")?;
            Ok(true)
        } else {
            Ok(false)
        }
    })?;
    Ok((binding, input))
}

fn parse_terminal_binding_extra<T>(
    parser: &mut Parser,
    mut extra: impl FnMut(&str, &mut Parser) -> Result<T, CliError>,
) -> Result<(TerminalBindingArgs, ()), CliError>
where
    T: Into<bool>,
{
    let mut attachment = None;
    let mut process = None;
    let mut originating_request = None;
    while let Some(option_owned) = parser.peek_utf8()?.map(str::to_owned) {
        let option = option_owned.as_str();
        match option {
            "--attachment" => {
                parser.pop();
                let value = parser.value_utf8("--attachment")?;
                set_once(&mut attachment, parse_hex_id(&value, "--attachment")?, "--attachment")?;
            }
            "--process" => {
                parser.pop();
                let value = parser.value_utf8("--process")?;
                set_once(&mut process, parse_hex_id(&value, "--process")?, "--process")?;
            }
            "--originating-request" => {
                parser.pop();
                let value = parser.value_utf8("--originating-request")?;
                set_once(
                    &mut originating_request,
                    parse_hex_id(&value, "--originating-request")?,
                    "--originating-request",
                )?;
            }
            _ if extra(option, parser)?.into() => {}
            _ => return Err(CliError::usage(format!("unknown terminal option: {option}"))),
        }
    }
    Ok((
        TerminalBindingArgs {
            attachment: required(attachment, "--attachment")?,
            process: required(process, "--process")?,
            originating_request: required(originating_request, "--originating-request")?,
        },
        (),
    ))
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
