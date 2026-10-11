//! Source-specific parsing failures and explicit command inventories.
use super::super::cargo_manifest_commands;
use peritus_app_protocol::{InitCommand, InitCommandKind, InitCommandVerification};
use std::path::Path;

pub(super) fn commands_from_source(source: &str, bytes: &[u8]) -> Result<Vec<InitCommand>, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("source is not UTF-8: {error}"))?;
    let path = Path::new(source);
    let leaf = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let mut commands = Vec::new();
    match leaf {
        "Cargo.toml" => {
            text.parse::<toml::Table>()
                .map_err(|error| format!("manifest could not be parsed: {error}"))?;
            cargo_manifest_commands(source, bytes, &mut commands)
                .map_err(|error| error.to_string())?;
            if source != "Cargo.toml" {
                commands = commands
                    .iter()
                    .map(|command| {
                        let mut arguments = command.arguments().to_vec();
                        arguments.splice(1..1, ["--manifest-path".to_owned(), source.to_owned()]);
                        InitCommand::new(
                            command.kind(),
                            source.to_owned(),
                            "cargo".to_owned(),
                            arguments,
                            InitCommandVerification::Unverified,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<_, _>>()?;
            }
        }
        "package.json" => {
            let document: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|error| format!("manifest could not be parsed: {error}"))?;
            if let Some(scripts) = document.get("scripts").and_then(serde_json::Value::as_object) {
                for (name, value) in scripts {
                    if value.as_str().is_none_or(str::is_empty) {
                        continue;
                    }
                    let mut arguments = Vec::new();
                    if let Some(parent) =
                        path.parent().filter(|parent| !parent.as_os_str().is_empty())
                    {
                        arguments
                            .extend(["--prefix".to_owned(), parent.to_string_lossy().into_owned()]);
                    }
                    arguments.extend(["run".to_owned(), name.clone()]);
                    commands.push(
                        InitCommand::new(
                            classify(name),
                            source.to_owned(),
                            "npm".to_owned(),
                            arguments,
                            InitCommandVerification::Unverified,
                        )
                        .map_err(|error| error.to_string())?,
                    );
                }
            }
        }
        "Makefile" | "justfile" => {
            for line in text.lines() {
                if line.starts_with(char::is_whitespace) || line.starts_with('#') {
                    continue;
                }
                let Some((targets, _)) = line.split_once(':') else {
                    continue;
                };
                for target in targets.split_whitespace() {
                    if target.contains(['=', '$', '%', '.', '/']) {
                        continue;
                    }
                    let executable = if leaf == "Makefile" { "make" } else { "just" };
                    let flag = if leaf == "Makefile" { "-f" } else { "--justfile" };
                    commands.push(
                        InitCommand::new(
                            classify(target),
                            source.to_owned(),
                            executable.to_owned(),
                            vec![flag.to_owned(), source.to_owned(), target.to_owned()],
                            InitCommandVerification::Unverified,
                        )
                        .map_err(|error| error.to_string())?,
                    );
                }
            }
        }
        "config" | "config.toml"
            if path.parent().is_some_and(|parent| parent.ends_with(".cargo")) =>
        {
            let document = text
                .parse::<toml::Table>()
                .map_err(|error| format!("command configuration could not be parsed: {error}"))?;
            if let Some(aliases) = document.get("alias").and_then(toml::Value::as_table) {
                for name in aliases.keys() {
                    commands.push(
                        InitCommand::new(
                            classify(name),
                            source.to_owned(),
                            "cargo".to_owned(),
                            vec!["--config".to_owned(), source.to_owned(), name.clone()],
                            InitCommandVerification::Unverified,
                        )
                        .map_err(|error| error.to_string())?,
                    );
                }
            }
        }
        "pyproject.toml" => {
            text.parse::<toml::Table>()
                .map_err(|error| format!("manifest could not be parsed: {error}"))?;
        }
        _ => return Err(
            "no command parser is available for this selected source; its exact bytes are retained"
                .to_owned(),
        ),
    }
    Ok(commands)
}
fn classify(name: &str) -> InitCommandKind {
    match name {
        "test" | "check" => InitCommandKind::Test,
        "lint" | "clippy" => InitCommandKind::Lint,
        "run" | "start" | "dev" => InitCommandKind::Launch,
        _ => InitCommandKind::Build,
    }
}
