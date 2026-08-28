//! Parsed CLI command and argument values.

use std::{ffi::OsString, path::PathBuf, time::Duration};

use peritus_types::SessionId;

use crate::completion::Shell;

pub const HELP: &str = r"peritus - interactive coding-agent harness and scriptable daemon client

USAGE:
  peritus                              Launch or resume the interactive product
  peritus [GLOBAL OPTIONS] <COMMAND>

GLOBAL OPTIONS:
  --endpoint <PATH-OR-PIPE>   Protected peritusd local endpoint
  --session <HEX-ID>         Resume a durable 128-bit session
  --timeout-seconds <N>      Connect/request timeout (default: 30)
  --json                     Emit stable JSON; streams use one object per line
  -h, --help                 Print this help
  -V, --version              Print version

COMMANDS:
  providers                    Open provider settings
  workspaces                   Switch, add, trust, repair, or forget workspaces
  open [PATH]                  Launch Peritus for PATH (default: current directory)
  status
  shutdown [--wait]
  command submit --actor <ID> --envelope <FILE> --payload <FILE>
                 --idempotency-key <KEY> [--no-expected-revision]
  events watch --topic <TOPIC>... [--after <CURSOR>] [--window <N>]
               [--count <N>] [--snapshot-acceptable]
  artifact get --artifact <ID> --output <FILE> [--force]
  artifact put --artifact <ID> --input <FILE> --media-type <TYPE>
               [--chunk-size <BYTES>]
  artifact cancel --transfer <ID> --artifact <ID>
  prompt answer --binding <FILE> (--signed-decision <FILE> | --text <TEXT> |
                --selection <ID> | --confirm <true|false> | --secret-reference <REF>)
                [--rationale <TEXT>]
  prompt cancel --binding <FILE>
  terminal attach --process <ID> [--no-follow]
  terminal input --attachment <ID> --process <ID> --originating-request <ID>
                 --input <FILE|->
  terminal resize --attachment <ID> --process <ID> --originating-request <ID>
                  --columns <N> --rows <N>
  terminal detach --attachment <ID> --process <ID> --originating-request <ID>
  terminal cancel --attachment <ID> --process <ID> --originating-request <ID>
  completions <bash|zsh|fish|powershell>

EXIT CATEGORIES:
  0 success; 2 usage; 10 connection; 11 negotiation; 12 daemon rejection;
  13 local I/O; 14 protocol; 70 internal; 130 interrupted
";

pub struct Cli {
    pub(crate) endpoint: Option<OsString>,
    pub(crate) session: Option<SessionId>,
    pub(crate) timeout: Duration,
    pub(crate) json: bool,
    pub(crate) command: Command,
}

pub enum Command {
    Help { text: String },
    Version,
    Completions(Shell),
    Providers,
    Workspaces,
    Open { path: Option<PathBuf> },
    Status,
    Shutdown { wait: bool },
    Submit(SubmitArgs),
    Events(EventArgs),
    ArtifactGet(ArtifactGetArgs),
    ArtifactPut(ArtifactPutArgs),
    ArtifactCancel(ArtifactCancelArgs),
    PromptAnswer(PromptAnswerArgs),
    PromptCancel(PromptCancelArgs),
    TerminalAttach(TerminalAttachArgs),
    TerminalInput(TerminalInputArgs),
    TerminalResize(TerminalResizeArgs),
    TerminalDetach(TerminalBindingArgs),
    TerminalCancel(TerminalBindingArgs),
}

pub struct SubmitArgs {
    pub(crate) actor: [u8; 16],
    pub(crate) envelope: PathBuf,
    pub(crate) payload: PathBuf,
    pub(crate) idempotency_key: Vec<u8>,
    pub(crate) bind_expected_revision: bool,
}

pub struct EventArgs {
    pub(crate) topics: Vec<String>,
    pub(crate) after: u64,
    pub(crate) window: u32,
    pub(crate) count: Option<u64>,
    pub(crate) snapshot_acceptable: bool,
}

pub struct ArtifactGetArgs {
    pub(crate) artifact: [u8; 16],
    pub(crate) output: PathBuf,
    pub(crate) force: bool,
}

pub struct ArtifactPutArgs {
    pub(crate) artifact: [u8; 16],
    pub(crate) input: PathBuf,
    pub(crate) media_type: String,
    pub(crate) chunk_size: u32,
}

pub struct ArtifactCancelArgs {
    pub(crate) transfer: [u8; 16],
    pub(crate) artifact: [u8; 16],
}

pub enum PromptValue {
    SignedDecision(PathBuf),
    Text(String),
    Selection(String),
    Confirmation(bool),
    SecretReference(String),
}

pub struct PromptAnswerArgs {
    pub(crate) binding: PathBuf,
    pub(crate) value: PromptValue,
    pub(crate) rationale: Option<String>,
}

pub struct PromptCancelArgs {
    pub(crate) binding: PathBuf,
}

pub struct TerminalAttachArgs {
    pub(crate) process: [u8; 16],
    pub(crate) follow: bool,
}

pub struct TerminalBindingArgs {
    pub(crate) attachment: [u8; 16],
    pub(crate) process: [u8; 16],
    pub(crate) originating_request: [u8; 16],
}

pub struct TerminalInputArgs {
    pub(crate) binding: TerminalBindingArgs,
    pub(crate) input: OsString,
}

pub struct TerminalResizeArgs {
    pub(crate) binding: TerminalBindingArgs,
    pub(crate) columns: u16,
    pub(crate) rows: u16,
}
