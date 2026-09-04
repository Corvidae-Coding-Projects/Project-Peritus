//! Terminal command-family parsing.

use std::ffi::OsString;

use super::{
    Command, Parser, TerminalAttachArgs, TerminalBindingArgs, TerminalInputArgs,
    TerminalResizeArgs, positive_u16, required, set_once,
};
use crate::{error::CliError, id::parse_hex_id};

pub(super) fn parse_terminal(parser: &mut Parser) -> Result<Command, CliError> {
    match parser.command("terminal subcommand")?.as_str() {
        "attach" => parse_attach(parser),
        "input" => parse_input(parser),
        "resize" => parse_resize(parser),
        "detach" => Ok(Command::TerminalDetach(parse_binding(parser)?)),
        "cancel" => Ok(Command::TerminalCancel(parse_binding(parser)?)),
        value => Err(CliError::usage(format!("unknown terminal subcommand: {value}"))),
    }
}

fn parse_attach(parser: &mut Parser) -> Result<Command, CliError> {
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

fn parse_input(parser: &mut Parser) -> Result<Command, CliError> {
    let (binding, input) = parse_binding_with(parser, true)?;
    Ok(Command::TerminalInput(TerminalInputArgs {
        binding,
        input: input.ok_or_else(|| CliError::usage("missing required --input"))?,
    }))
}

fn parse_resize(parser: &mut Parser) -> Result<Command, CliError> {
    let mut columns = None;
    let mut rows = None;
    let binding = parse_binding_extra(parser, |option, parser| match option {
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

fn parse_binding(parser: &mut Parser) -> Result<TerminalBindingArgs, CliError> {
    parse_binding_with(parser, false).map(|(binding, _)| binding)
}

fn parse_binding_with(
    parser: &mut Parser,
    allow_input: bool,
) -> Result<(TerminalBindingArgs, Option<OsString>), CliError> {
    let mut input = None;
    let binding = parse_binding_extra(parser, |option, parser| {
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

fn parse_binding_extra(
    parser: &mut Parser,
    mut extra: impl FnMut(&str, &mut Parser) -> Result<bool, CliError>,
) -> Result<TerminalBindingArgs, CliError> {
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
            _ if extra(option, parser)? => {}
            _ => return Err(CliError::usage(format!("unknown terminal option: {option}"))),
        }
    }
    Ok(TerminalBindingArgs {
        attachment: required(attachment, "--attachment")?,
        process: required(process, "--process")?,
        originating_request: required(originating_request, "--originating-request")?,
    })
}
