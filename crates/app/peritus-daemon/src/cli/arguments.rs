//! Closed command-line vocabulary and exact argument parsing.

use std::ffi::{OsStr, OsString};

pub(super) enum CommandLine {
    Version,
    Serve(OsString),
    QualifyPty,
    StageBlobBeforeCrash(OsString),
    RecoverBlobBeforeCrash(OsString),
    StageBlobAfterCrash(OsString),
    RecoverBlobAfterCrash(OsString),
    StageJournalBeforeCrash(OsString),
    RecoverJournalBeforeCrash(OsString),
    StageSnapshotBeforeCrash(OsString),
    RecoverSnapshotBeforeCrash(OsString),
    StageSnapshotAfterCrash(OsString),
    RecoverSnapshotAfterCrash(OsString),
    StageLeaseBeforeCrash(OsString),
    RecoverLeaseBeforeCrash(OsString),
    StageLeaseAfterCrash(OsString),
    RecoverLeaseAfterCrash(OsString),
    StagePatchBeforeCrash(OsString),
    RecoverPatchBeforeCrash(OsString),
    StagePatchAfterCrash(OsString),
    RecoverPatchAfterCrash(OsString),
    StageGateBeforeCrash(OsString),
    RecoverGateBeforeCrash(OsString),
    StageGateAfterCrash(OsString),
    RecoverGateAfterCrash(OsString),
    StageOutboxCrash(OsString),
    RecoverOutboxCrash(OsString),
}

pub(super) fn parse(arguments: &mut impl Iterator<Item = OsString>) -> Option<CommandLine> {
    let command = arguments.next()?;
    match command.to_str()? {
        "--version" if arguments.next().is_none() => Some(CommandLine::Version),
        "serve" => configuration_argument(arguments).map(CommandLine::Serve),
        "qualify-pty" if arguments.next().is_none() => Some(CommandLine::QualifyPty),
        "qualify-blob-before-stage" => configured(arguments, CommandLine::StageBlobBeforeCrash),
        "qualify-blob-before-recover" => configured(arguments, CommandLine::RecoverBlobBeforeCrash),
        "qualify-blob-after-stage" => configured(arguments, CommandLine::StageBlobAfterCrash),
        "qualify-blob-after-recover" => configured(arguments, CommandLine::RecoverBlobAfterCrash),
        "qualify-journal-before-stage" => {
            configured(arguments, CommandLine::StageJournalBeforeCrash)
        }
        "qualify-journal-before-recover" => {
            configured(arguments, CommandLine::RecoverJournalBeforeCrash)
        }
        "qualify-snapshot-before-stage" => {
            configured(arguments, CommandLine::StageSnapshotBeforeCrash)
        }
        "qualify-snapshot-before-recover" => {
            configured(arguments, CommandLine::RecoverSnapshotBeforeCrash)
        }
        "qualify-snapshot-after-stage" => {
            configured(arguments, CommandLine::StageSnapshotAfterCrash)
        }
        "qualify-snapshot-after-recover" => {
            configured(arguments, CommandLine::RecoverSnapshotAfterCrash)
        }
        "qualify-lease-before-stage" => configured(arguments, CommandLine::StageLeaseBeforeCrash),
        "qualify-lease-before-recover" => {
            configured(arguments, CommandLine::RecoverLeaseBeforeCrash)
        }
        "qualify-lease-after-stage" => configured(arguments, CommandLine::StageLeaseAfterCrash),
        "qualify-lease-after-recover" => configured(arguments, CommandLine::RecoverLeaseAfterCrash),
        "qualify-patch-before-stage" => configured(arguments, CommandLine::StagePatchBeforeCrash),
        "qualify-patch-before-recover" => {
            configured(arguments, CommandLine::RecoverPatchBeforeCrash)
        }
        "qualify-patch-after-stage" => configured(arguments, CommandLine::StagePatchAfterCrash),
        "qualify-patch-after-recover" => configured(arguments, CommandLine::RecoverPatchAfterCrash),
        "qualify-gate-before-stage" => configured(arguments, CommandLine::StageGateBeforeCrash),
        "qualify-gate-before-recover" => configured(arguments, CommandLine::RecoverGateBeforeCrash),
        "qualify-gate-after-stage" => configured(arguments, CommandLine::StageGateAfterCrash),
        "qualify-gate-after-recover" => configured(arguments, CommandLine::RecoverGateAfterCrash),
        "qualify-outbox-stage" => configured(arguments, CommandLine::StageOutboxCrash),
        "qualify-outbox-recover" => configured(arguments, CommandLine::RecoverOutboxCrash),
        _ => None,
    }
}

fn configured(
    arguments: &mut impl Iterator<Item = OsString>,
    constructor: fn(OsString) -> CommandLine,
) -> Option<CommandLine> {
    configuration_argument(arguments).map(constructor)
}

fn configuration_argument(arguments: &mut impl Iterator<Item = OsString>) -> Option<OsString> {
    let flag = arguments.next()?;
    let configuration = arguments.next()?;
    (flag == OsStr::new("--config") && arguments.next().is_none()).then_some(configuration)
}
