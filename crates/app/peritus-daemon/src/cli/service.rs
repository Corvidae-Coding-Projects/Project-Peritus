//! Typed production service commands and their exact private child boundary.

use std::{
    ffi::{OsStr, OsString},
    process::ExitCode,
};

pub(super) enum Command {
    Serve(OsString),
    Supervise(OsString),
    Stop(OsString),
    PackageHandoff(OsString),
    PackageHandoffLegacy(OsString),
    PackageAdopt {
        store: OsString,
        candidate: OsString,
        current: OsString,
        configuration: Option<OsString>,
        retired: Option<OsString>,
    },
    PackageRemove {
        store: OsString,
        current: OsString,
        expected: OsString,
        configuration: Option<OsString>,
    },
    PackageLock { lock: OsString, command: Vec<OsString> },
    SupervisedServe { configuration: OsString, owner_token: OsString },
}

pub(super) fn parse(
    command: &OsStr,
    arguments: &mut impl Iterator<Item = OsString>,
) -> Option<Command> {
    match command.to_str()? {
        "serve" => configuration_argument(arguments).map(Command::Serve),
        "supervise" => configuration_argument(arguments).map(Command::Supervise),
        "supervise-stop" => configuration_argument(arguments).map(Command::Stop),
        "package-handoff" => configuration_argument(arguments).map(Command::PackageHandoff),
        "package-handoff-legacy" => {
            configuration_argument(arguments).map(Command::PackageHandoffLegacy)
        }
        "package-adopt" => package_adopt(arguments),
        "package-remove" => package_remove(arguments),
        "package-lock" => package_lock(arguments),
        "--supervised-serve-v1" => supervised_serve(arguments),
        _ => None,
    }
}

pub(super) fn run(command: Command) -> ExitCode {
    match command {
        Command::Serve(configuration) => super::server::run(configuration),
        Command::Supervise(configuration) => super::supervisor::run(configuration),
        Command::Stop(configuration) => super::supervisor::request_stop(configuration),
        Command::PackageHandoff(configuration) => package_handoff(configuration),
        Command::PackageHandoffLegacy(configuration) => package_handoff_legacy(configuration),
        Command::PackageAdopt { store, candidate, current, configuration, retired } => {
            super::package::adopt(store, candidate, current, configuration, retired)
        }
        Command::PackageRemove { store, current, expected, configuration } => {
            super::package::remove(store, current, expected, configuration)
        }
        Command::PackageLock { lock, command } => super::package::locked_run(lock, command),
        Command::SupervisedServe { configuration, owner_token } => {
            super::supervisor::run_server(configuration, owner_token)
        }
    }
}

fn package_lock(arguments: &mut impl Iterator<Item = OsString>) -> Option<Command> {
    let lock_flag = arguments.next()?;
    let lock = arguments.next()?;
    let separator = arguments.next()?;
    if lock_flag != OsStr::new("--lock") || separator != OsStr::new("--") {
        return None;
    }
    let command = arguments.collect::<Vec<_>>();
    (!command.is_empty()).then_some(Command::PackageLock { lock, command })
}

fn package_adopt(arguments: &mut impl Iterator<Item = OsString>) -> Option<Command> {
    let store_flag = arguments.next()?;
    let store = arguments.next()?;
    let candidate_flag = arguments.next()?;
    let candidate = arguments.next()?;
    let current_flag = arguments.next()?;
    let current = arguments.next()?;
    let mut configuration = None;
    let mut retired = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next()?;
        if flag == OsStr::new("--config") && configuration.is_none() {
            configuration = Some(value);
        } else if flag == OsStr::new("--retired") && retired.is_none() {
            retired = Some(value);
        } else {
            return None;
        }
    }
    if store_flag != OsStr::new("--store")
        || candidate_flag != OsStr::new("--candidate")
        || current_flag != OsStr::new("--current")
    {
        return None;
    }
    Some(Command::PackageAdopt { store, candidate, current, configuration, retired })
}

fn package_remove(arguments: &mut impl Iterator<Item = OsString>) -> Option<Command> {
    let store_flag = arguments.next()?;
    let store = arguments.next()?;
    let current_flag = arguments.next()?;
    let current = arguments.next()?;
    let expected_flag = arguments.next()?;
    let expected = arguments.next()?;
    let configuration = match arguments.next() {
        Some(flag) if flag == OsStr::new("--config") => Some(arguments.next()?),
        Some(_) => return None,
        None => None,
    };
    if store_flag != OsStr::new("--store")
        || current_flag != OsStr::new("--current")
        || expected_flag != OsStr::new("--expected")
        || arguments.next().is_some()
    {
        return None;
    }
    Some(Command::PackageRemove { store, current, expected, configuration })
}

fn package_handoff(configuration: OsString) -> ExitCode {
    package_handoff_with(configuration, crate::instance::request_handoff)
}

fn package_handoff_legacy(configuration: OsString) -> ExitCode {
    package_handoff_with(configuration, crate::instance::request_legacy_handoff)
}

fn package_handoff_with(
    configuration: OsString,
    operation: fn(
        &crate::DaemonConfig,
    ) -> Result<crate::instance::HandoffCompletion, crate::DaemonError>,
) -> ExitCode {
    let config = match crate::DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => {
            super::write_error(&error.to_string());
            return ExitCode::FAILURE;
        }
    };
    match operation(&config) {
        Ok(completion) => match std::str::from_utf8(completion.bytes()) {
            Ok(text) => super::write_output(text.trim_end()).map_or_else(
                |error| {
                    super::write_error(&format!("failed to write package handoff receipt: {error}"));
                    ExitCode::FAILURE
                },
                |()| ExitCode::SUCCESS,
            ),
            Err(error) => {
                super::write_error(&format!("invalid package handoff receipt: {error}"));
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            super::write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

fn configuration_argument(arguments: &mut impl Iterator<Item = OsString>) -> Option<OsString> {
    let flag = arguments.next()?;
    let configuration = arguments.next()?;
    (flag == OsStr::new("--config") && arguments.next().is_none()).then_some(configuration)
}

fn supervised_serve(arguments: &mut impl Iterator<Item = OsString>) -> Option<Command> {
    let config_flag = arguments.next()?;
    let configuration = arguments.next()?;
    let token_flag = arguments.next()?;
    let owner_token = arguments.next()?;
    if config_flag != OsStr::new("--config")
        || token_flag != OsStr::new("--owner-token")
        || arguments.next().is_some()
    {
        return None;
    }
    Some(Command::SupervisedServe { configuration, owner_token })
}
