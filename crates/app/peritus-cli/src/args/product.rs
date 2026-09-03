//! Product-run command parsing kept separate from the general CLI grammar.

use peritus_app_protocol::ProductRunControlAction;
use peritus_types::RunId;

use super::{Command, Parser, ProductRunArgs, required, set_once};
use crate::{
    error::CliError,
    id::{parse_hex_digest, parse_hex_id},
};

pub(super) fn parse_product(parser: &mut Parser) -> Result<Command, CliError> {
    let subcommand = parser.command("runs subcommand")?;
    let arguments = match subcommand.as_str() {
        "list" => {
            parser.finish()?;
            ProductRunArgs::List
        }
        "show" => ProductRunArgs::Show { run_id: run_id(parser)? },
        "continue" => continuation(parser)?,
        "execute" => ProductRunArgs::Execute { run_id: run_id(parser)? },
        "accept" => control(parser, ProductRunControlAction::Accept, true)?,
        "commit" => control(parser, ProductRunControlAction::Commit, true)?,
        "export" => control(parser, ProductRunControlAction::Export, false)?,
        "discard" => control(parser, ProductRunControlAction::Discard, false)?,
        "retry" => control(parser, ProductRunControlAction::Retry, false)?,
        "cancel" => control(parser, ProductRunControlAction::Cancel, false)?,
        _ => return Err(CliError::usage(format!("unknown runs subcommand: {subcommand}"))),
    };
    Ok(Command::ProductRuns(arguments))
}

fn run_id(parser: &mut Parser) -> Result<RunId, CliError> {
    let mut run = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--run" => {
                parser.pop();
                let value = parser.value_utf8("--run")?;
                let value = parse_hex_id(&value, "--run")?;
                set_once(&mut run, value, "--run")?;
            }
            _ => return Err(CliError::usage(format!("unknown runs option: {option}"))),
        }
    }
    RunId::new(required(run, "--run")?)
        .map_err(|_| CliError::usage("--run cannot be the all-zero identifier"))
}

fn continuation(parser: &mut Parser) -> Result<ProductRunArgs, CliError> {
    let mut run = None;
    let mut message = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--run" => {
                parser.pop();
                let value = parser.value_utf8("--run")?;
                set_once(&mut run, parse_hex_id(&value, "--run")?, "--run")?;
            }
            "--message" => {
                parser.pop();
                set_once(&mut message, parser.value_utf8("--message")?, "--message")?;
            }
            _ => return Err(CliError::usage(format!("unknown runs continue option: {option}"))),
        }
    }
    let run_id = RunId::new(required(run, "--run")?)
        .map_err(|_| CliError::usage("--run cannot be the all-zero identifier"))?;
    Ok(ProductRunArgs::Continue { run_id, message: required(message, "--message")? })
}

fn control(
    parser: &mut Parser,
    action: ProductRunControlAction,
    confirmation_allowed: bool,
) -> Result<ProductRunArgs, CliError> {
    let mut run = None;
    let mut confirmed_digest = None;
    while let Some(option) = parser.peek_utf8()? {
        match option {
            "--run" => {
                parser.pop();
                let value = parser.value_utf8("--run")?;
                set_once(&mut run, parse_hex_id(&value, "--run")?, "--run")?;
            }
            "--confirm-unqualified" if confirmation_allowed => {
                parser.pop();
                let value = parser.value_utf8("--confirm-unqualified")?;
                set_once(
                    &mut confirmed_digest,
                    parse_hex_digest(&value, "--confirm-unqualified")?,
                    "--confirm-unqualified",
                )?;
            }
            _ => return Err(CliError::usage(format!("unknown runs control option: {option}"))),
        }
    }
    let run_id = RunId::new(required(run, "--run")?)
        .map_err(|_| CliError::usage("--run cannot be the all-zero identifier"))?;
    Ok(ProductRunArgs::Control { run_id, action, confirmed_digest })
}
