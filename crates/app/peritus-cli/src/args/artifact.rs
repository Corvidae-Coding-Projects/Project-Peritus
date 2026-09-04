//! Artifact command-family parsing.

use super::{
    ArtifactCancelArgs, ArtifactGetArgs, ArtifactPutArgs, Command, Parser, positive_u32, required,
    set_once,
};
use crate::{error::CliError, id::parse_hex_id};

pub(super) fn parse_artifact(parser: &mut Parser) -> Result<Command, CliError> {
    match parser.command("artifact subcommand")?.as_str() {
        "get" => parse_get(parser),
        "put" => parse_put(parser),
        "cancel" => parse_cancel(parser),
        value => Err(CliError::usage(format!("unknown artifact subcommand: {value}"))),
    }
}

fn parse_cancel(parser: &mut Parser) -> Result<Command, CliError> {
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

fn parse_get(parser: &mut Parser) -> Result<Command, CliError> {
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

fn parse_put(parser: &mut Parser) -> Result<Command, CliError> {
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
