//! Real `peritusd` subprocess bridge for terminal-ordering qualification.

use std::io;
use std::process::{Command, Stdio};
use std::str::FromStr;

use peritus_conformance::{DaemonConformanceObservation, DaemonTerminalObservation};

const OBSERVATION_PREFIX: &str = "peritus-qualification pty";

/// Exercises the production `qualify-pty` command and parses its stable observation line.
pub(super) fn pty_ordering() -> io::Result<DaemonConformanceObservation> {
    let output = Command::new(super::process::peritusd_executable()?)
        .arg("qualify-pty")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "peritusd qualify-pty exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        )));
    }
    let stdout = std::str::from_utf8(&output.stdout).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("peritusd qualify-pty output was not UTF-8: {error}"),
        )
    })?;
    Ok(DaemonConformanceObservation::Terminal(parse_observation(stdout)?))
}

fn parse_observation(stdout: &str) -> io::Result<DaemonTerminalObservation> {
    let mut lines = stdout.lines();
    let line = lines
        .next()
        .ok_or_else(|| invalid_data("peritusd qualify-pty returned no observation line"))?;
    if lines.next().is_some() {
        return Err(invalid_data("peritusd qualify-pty returned more than one observation line"));
    }
    let fields = line
        .strip_prefix(OBSERVATION_PREFIX)
        .filter(|suffix| suffix.starts_with(' '))
        .ok_or_else(|| invalid_data("peritusd qualify-pty returned an unknown observation"))?;
    let mut fields = fields.split_ascii_whitespace();
    let output_bytes = parse_field(&mut fields, "output_bytes")?;
    let sequence_strictly_increasing = parse_field(&mut fields, "sequence_strictly_increasing")?;
    let offsets_conserved = parse_field(&mut fields, "offsets_conserved")?;
    let combined_stream_only = parse_field(&mut fields, "combined_stream_only")?;
    let exit_records = parse_field(&mut fields, "exit_records")?;
    let peak_buffered_bytes = parse_field(&mut fields, "peak_buffered_bytes")?;
    let configured_buffer_limit = parse_field(&mut fields, "configured_buffer_limit")?;
    if fields.next().is_some() {
        return Err(invalid_data(
            "peritusd qualify-pty observation had unexpected trailing fields",
        ));
    }
    Ok(DaemonTerminalObservation::new(
        output_bytes,
        sequence_strictly_increasing,
        offsets_conserved,
        combined_stream_only,
        exit_records,
        peak_buffered_bytes,
        configured_buffer_limit,
    ))
}

fn parse_field<'a, T>(
    fields: &mut impl Iterator<Item = &'a str>,
    expected_name: &'static str,
) -> io::Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let field = fields
        .next()
        .ok_or_else(|| invalid_data(format!("missing qualify-pty field `{expected_name}`")))?;
    let (name, value) = field
        .split_once('=')
        .ok_or_else(|| invalid_data(format!("malformed qualify-pty field `{expected_name}`")))?;
    if name != expected_name {
        return Err(invalid_data(format!(
            "expected qualify-pty field `{expected_name}`, found `{name}`",
        )));
    }
    value.parse().map_err(|error| {
        invalid_data(format!("invalid qualify-pty field `{expected_name}`: {error}"))
    })
}

fn invalid_data(detail: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail.into())
}
