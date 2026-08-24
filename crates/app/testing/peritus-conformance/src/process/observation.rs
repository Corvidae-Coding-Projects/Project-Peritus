//! Raw observations returned by C2 process conformance adapters.

use super::{
    ProcessDisposition, ProcessRecoveryDisposition, ProcessStreamOffsetObservation, ProcessTrigger,
};

/// Exact invocation data observed by the helper process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessInvocationObservation {
    command: Vec<String>,
    working_directory: String,
    environment: Vec<(String, String)>,
    shell_interpreted: bool,
}

impl ProcessInvocationObservation {
    /// Creates one complete invocation observation.
    #[must_use]
    pub const fn new(
        command: Vec<String>,
        working_directory: String,
        environment: Vec<(String, String)>,
        shell_interpreted: bool,
    ) -> Self {
        Self { command, working_directory, environment, shell_interpreted }
    }

    /// Returns executable followed by literal argv.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }
    /// Returns the observed working directory.
    #[must_use]
    pub fn working_directory(&self) -> &str {
        &self.working_directory
    }
    /// Returns the complete observed child environment.
    #[must_use]
    pub fn environment(&self) -> &[(String, String)] {
        &self.environment
    }
    /// Returns whether any shell parsed the structured command.
    #[must_use]
    pub const fn shell_interpreted(&self) -> bool {
        self.shell_interpreted
    }
}

/// Bounded stream and event observations.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "input, resize, and completeness are independent stream observations"
)]
pub struct ProcessOutputObservation {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    terminal: Vec<u8>,
    event_sequences: Vec<u64>,
    stream_offsets: Vec<ProcessStreamOffsetObservation>,
    observed_bytes: u64,
    retained_bytes: u64,
    dropped_bytes: u64,
    complete: bool,
    input_closed: bool,
    resize_observed: bool,
}

impl ProcessOutputObservation {
    /// Creates one complete bounded-output observation.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        clippy::too_many_arguments,
        reason = "stream accounting dimensions are independently asserted"
    )]
    pub const fn new(
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        terminal: Vec<u8>,
        event_sequences: Vec<u64>,
        stream_offsets: Vec<ProcessStreamOffsetObservation>,
        observed_bytes: u64,
        retained_bytes: u64,
        dropped_bytes: u64,
        complete: bool,
        input_closed: bool,
        resize_observed: bool,
    ) -> Self {
        Self {
            stdout,
            stderr,
            terminal,
            event_sequences,
            stream_offsets,
            observed_bytes,
            retained_bytes,
            dropped_bytes,
            complete,
            input_closed,
            resize_observed,
        }
    }

    /// Returns retained standard output.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }
    /// Returns retained standard error.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
    /// Returns retained combined terminal output.
    #[must_use]
    pub fn terminal(&self) -> &[u8] {
        &self.terminal
    }
    /// Returns emitted event sequences.
    #[must_use]
    pub fn event_sequences(&self) -> &[u64] {
        &self.event_sequences
    }
    /// Returns stream offsets in observation order.
    #[must_use]
    pub fn stream_offsets(&self) -> &[ProcessStreamOffsetObservation] {
        &self.stream_offsets
    }
    /// Returns all bytes observed before policy accounting.
    #[must_use]
    pub const fn observed_bytes(&self) -> u64 {
        self.observed_bytes
    }
    /// Returns retained bytes.
    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
    /// Returns explicitly dropped bytes.
    #[must_use]
    pub const fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }
    /// Returns whether the retained output is complete.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    /// Returns whether the input owner observed closure.
    #[must_use]
    pub const fn input_closed(&self) -> bool {
        self.input_closed
    }
    /// Returns whether a requested PTY resize was observed.
    #[must_use]
    pub const fn resize_observed(&self) -> bool {
        self.resize_observed
    }
}

/// Process-tree and support-task observations at publication time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "tree, task, graceful, and forced observations are independently asserted"
)]
pub struct ProcessOwnershipObservation {
    descendants_observed: u64,
    tree_quiescent: bool,
    support_tasks_joined: bool,
    terminal_records: u64,
    graceful_stop_observed: bool,
    forced_stop_observed: bool,
}

impl ProcessOwnershipObservation {
    /// Creates one ownership observation.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "ownership completion and escalation observations remain independent"
    )]
    pub const fn new(
        descendants_observed: u64,
        tree_quiescent: bool,
        support_tasks_joined: bool,
        terminal_records: u64,
        graceful_stop_observed: bool,
        forced_stop_observed: bool,
    ) -> Self {
        Self {
            descendants_observed,
            tree_quiescent,
            support_tasks_joined,
            terminal_records,
            graceful_stop_observed,
            forced_stop_observed,
        }
    }

    /// Returns the owned descendants observed.
    #[must_use]
    pub const fn descendants_observed(&self) -> u64 {
        self.descendants_observed
    }
    /// Returns whether the complete owned tree is quiescent.
    #[must_use]
    pub const fn tree_quiescent(&self) -> bool {
        self.tree_quiescent
    }
    /// Returns whether every support task joined.
    #[must_use]
    pub const fn support_tasks_joined(&self) -> bool {
        self.support_tasks_joined
    }
    /// Returns accepted terminal-record count.
    #[must_use]
    pub const fn terminal_records(&self) -> u64 {
        self.terminal_records
    }
    /// Returns whether graceful stop was attempted.
    #[must_use]
    pub const fn graceful_stop_observed(&self) -> bool {
        self.graceful_stop_observed
    }
    /// Returns whether forced escalation was attempted.
    #[must_use]
    pub const fn forced_stop_observed(&self) -> bool {
        self.forced_stop_observed
    }
}

/// Effect counts used to prove rejected authority has no target effect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessEffectObservation {
    sandbox_activations: u64,
    process_launches: u64,
    spools_created: u64,
    authorization_consumed: bool,
}

impl ProcessEffectObservation {
    /// Creates exact target-effect counts.
    #[must_use]
    pub const fn new(
        sandbox_activations: u64,
        process_launches: u64,
        spools_created: u64,
        authorization_consumed: bool,
    ) -> Self {
        Self { sandbox_activations, process_launches, spools_created, authorization_consumed }
    }

    /// Returns sandbox activation count.
    #[must_use]
    pub const fn sandbox_activations(&self) -> u64 {
        self.sandbox_activations
    }
    /// Returns process launch count.
    #[must_use]
    pub const fn process_launches(&self) -> u64 {
        self.process_launches
    }
    /// Returns spool creation count.
    #[must_use]
    pub const fn spools_created(&self) -> u64 {
        self.spools_created
    }
    /// Returns whether the rejected authorization was consumed.
    #[must_use]
    pub const fn authorization_consumed(&self) -> bool {
        self.authorization_consumed
    }
    /// Returns whether any target effect was observed.
    #[must_use]
    pub const fn any_effect(&self) -> bool {
        self.sandbox_activations != 0 || self.process_launches != 0 || self.spools_created != 0
    }
}

/// Complete raw observation returned by a process conformance subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessConformanceObservation {
    disposition: ProcessDisposition,
    trigger: Option<ProcessTrigger>,
    invocation: ProcessInvocationObservation,
    output: ProcessOutputObservation,
    ownership: ProcessOwnershipObservation,
    effects: ProcessEffectObservation,
    recovery: Option<ProcessRecoveryDisposition>,
    success_inferred_without_terminal: bool,
    recovery_signal_sent: bool,
}

impl ProcessConformanceObservation {
    /// Creates one complete process observation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "terminal and recovery facts remain independent")]
    pub const fn new(
        disposition: ProcessDisposition,
        trigger: Option<ProcessTrigger>,
        invocation: ProcessInvocationObservation,
        output: ProcessOutputObservation,
        ownership: ProcessOwnershipObservation,
        effects: ProcessEffectObservation,
        recovery: Option<ProcessRecoveryDisposition>,
        success_inferred_without_terminal: bool,
        recovery_signal_sent: bool,
    ) -> Self {
        Self {
            disposition,
            trigger,
            invocation,
            output,
            ownership,
            effects,
            recovery,
            success_inferred_without_terminal,
            recovery_signal_sent,
        }
    }

    /// Returns the deterministic terminal disposition.
    #[must_use]
    pub const fn disposition(&self) -> ProcessDisposition {
        self.disposition
    }
    /// Returns the first accepted trigger, if any.
    #[must_use]
    pub const fn trigger(&self) -> Option<ProcessTrigger> {
        self.trigger
    }
    /// Borrows invocation observations.
    #[must_use]
    pub const fn invocation(&self) -> &ProcessInvocationObservation {
        &self.invocation
    }
    /// Borrows bounded-output observations.
    #[must_use]
    pub const fn output(&self) -> &ProcessOutputObservation {
        &self.output
    }
    /// Returns process ownership observations.
    #[must_use]
    pub const fn ownership(&self) -> ProcessOwnershipObservation {
        self.ownership
    }
    /// Returns target-effect observations.
    #[must_use]
    pub const fn effects(&self) -> ProcessEffectObservation {
        self.effects
    }
    /// Returns a restart classification when the fixture exercises recovery.
    #[must_use]
    pub const fn recovery(&self) -> Option<ProcessRecoveryDisposition> {
        self.recovery
    }
    /// Returns whether recovery guessed success without a terminal observation.
    #[must_use]
    pub const fn success_inferred_without_terminal(&self) -> bool {
        self.success_inferred_without_terminal
    }
    /// Returns whether recovery attempted to signal a resolved identity.
    #[must_use]
    pub const fn recovery_signal_sent(&self) -> bool {
        self.recovery_signal_sent
    }
}
