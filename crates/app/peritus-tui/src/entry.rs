//! Process entry and strict TUI argument parsing.

use std::{
    ffi::OsString,
    io::{self, Write as _},
    path::PathBuf,
    process::ExitCode,
};

use peritus_types::SessionId;

use crate::{TuiConfig, run};

/// Parses environment arguments and runs the interactive terminal client.
#[must_use]
pub fn run_env() -> ExitCode {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    let program = arguments.first().cloned().unwrap_or_else(|| OsString::from("peritus-tui"));
    match parse(&arguments[1..]) {
        Ok(ParseOutcome::Help) => {
            write_usage(&program, &mut io::stdout());
            ExitCode::SUCCESS
        }
        Ok(ParseOutcome::Run(config)) => run_config(config),
        Err(error) => {
            let _ = writeln!(io::stderr(), "peritus-tui: {error}");
            write_usage(&program, &mut io::stderr());
            ExitCode::from(2)
        }
    }
}

fn run_config(config: TuiConfig) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = writeln!(io::stderr(), "peritus-tui: could not start runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(config)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "peritus-tui: {error}");
            ExitCode::FAILURE
        }
    }
}

enum ParseOutcome {
    Help,
    Run(TuiConfig),
}

fn parse(arguments: &[OsString]) -> Result<ParseOutcome, String> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("-h" | "--help")) {
        return Ok(ParseOutcome::Help);
    }
    let mut endpoint = None;
    let mut session = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].to_str() {
            Some("--endpoint") => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--endpoint requires a local path or pipe name".to_owned())?;
                endpoint = Some(PathBuf::from(value));
            }
            Some("--session") => {
                index += 1;
                let value = arguments
                    .get(index)
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "--session requires 32 hexadecimal digits".to_owned())?;
                let bytes = decode_hex_16(value)
                    .ok_or_else(|| "--session requires 32 hexadecimal digits".to_owned())?;
                session = Some(
                    SessionId::new(bytes).map_err(|error| format!("invalid session: {error:?}"))?,
                );
            }
            Some(flag) => return Err(format!("unknown argument: {flag}")),
            None => return Err("arguments must be valid UTF-8 except the endpoint path".to_owned()),
        }
        index += 1;
    }
    let endpoint = endpoint.ok_or_else(|| "--endpoint is required".to_owned())?;
    let config = session.map_or_else(
        || TuiConfig::new(endpoint.clone()),
        |session| TuiConfig::new(endpoint.clone()).with_session(session),
    );
    Ok(ParseOutcome::Run(config))
}

fn decode_hex_16(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0_u8; 16];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        output[index] = hex_nibble(pair[0])? << 4 | hex_nibble(pair[1])?;
    }
    Some(output)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn write_usage(program: &OsString, writer: &mut impl io::Write) {
    let _ = writeln!(
        writer,
        "usage: {} --endpoint <unix-socket-or-windows-pipe> [--session <32-hex-digits>]",
        PathBuf::from(program).display()
    );
}
