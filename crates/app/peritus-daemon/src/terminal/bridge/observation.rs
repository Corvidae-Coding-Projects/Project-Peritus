//! Ordered C2 event observation and projection into every live A3 attachment.

use peritus_app_protocol::{TerminalExitDisposition, TerminalStream};
use peritus_process::{
    OsExitObservation, OutputStream, ProcessCursor, ProcessEvent, ProcessEventKind, TerminalResult,
};

use super::{ObservedOutput, TerminalBridge};
use crate::terminal::{TerminalBridgeError, TerminalBridgeErrorKind, TerminalRegistryLimits};

impl TerminalBridge {
    pub(in crate::terminal) fn observe(
        &mut self,
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if let Some((kind, detail)) = self.fault {
            return Err(rejected(kind, detail));
        }
        let mut drained = false;
        for _ in 0..limits.maximum_process_pages_per_poll() {
            let events = self
                .control
                .read_events(self.process_cursor, limits.maximum_process_events_per_page());
            if events.is_empty() {
                drained = true;
                break;
            }
            let page_is_short = events.len() < limits.maximum_process_events_per_page();
            for event in events {
                if let Err(error) = self.accept_process_event(&event, limits) {
                    self.fail(error.kind(), "live process event stream became unobservable");
                    return Err(error);
                }
                self.process_cursor = ProcessCursor::after(event.sequence());
            }
            if page_is_short {
                drained = true;
                break;
            }
        }
        if drained
            && self.terminal.is_none()
            && let Some(result) = self.control.terminal_result()
        {
            self.accept_terminal(result, limits)?;
        }
        Ok(())
    }

    fn accept_process_event(
        &mut self,
        event: &ProcessEvent,
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if event.process_id() != self.process_id || event.plan_digest() != self.plan_digest {
            return Err(rejected(
                TerminalBridgeErrorKind::ProcessIdentityMismatch,
                "process event identity does not match its live registration",
            ));
        }
        if event.sequence() != self.process_cursor.sequence().saturating_add(1) {
            return Err(rejected(
                TerminalBridgeErrorKind::ReplayUnavailable,
                "the bounded C2 process event window contains a sequence gap",
            ));
        }
        match event.kind() {
            ProcessEventKind::Started { root_pid }
                if *root_pid != self.birth_identity.root_pid() =>
            {
                Err(rejected(
                    TerminalBridgeErrorKind::ProcessIdentityMismatch,
                    "the started process identifier differs from its birth registration",
                ))
            }
            ProcessEventKind::Output(stream) => {
                self.accept_output(*stream, event.stream_offset(), event.data(), limits)
            }
            _ => Ok(()),
        }
    }

    fn accept_output(
        &mut self,
        stream: OutputStream,
        offset: Option<u64>,
        bytes: &[u8],
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if stream != OutputStream::Terminal {
            return Err(rejected(
                TerminalBridgeErrorKind::ProcessIdentityMismatch,
                "a registered PTY emitted a separated stdout or stderr stream",
            ));
        }
        let stream_index = stream_index(stream);
        if bytes.is_empty() || offset != Some(self.stream_offsets[stream_index]) {
            return Err(rejected(
                TerminalBridgeErrorKind::ReplayUnavailable,
                "process output is empty or has a noncontiguous stream offset",
            ));
        }
        let length = u64::try_from(bytes.len()).map_err(|_| {
            rejected(TerminalBridgeErrorKind::ReplayUnavailable, "process output length overflow")
        })?;
        let next_stream_offset =
            self.stream_offsets[stream_index].checked_add(length).ok_or_else(|| {
                rejected(TerminalBridgeErrorKind::ReplayUnavailable, "stream offset overflow")
            })?;
        let next_output_offset = self.next_output_offset.checked_add(length).ok_or_else(|| {
            rejected(TerminalBridgeErrorKind::ReplayUnavailable, "terminal output offset overflow")
        })?;
        let observed = ObservedOutput {
            offset: self.next_output_offset,
            stream: terminal_stream(stream),
            bytes: bytes.to_vec(),
        };
        for attachment in self.attachments.values_mut() {
            attachment.enqueue_output(&observed, limits);
        }
        self.stream_offsets[stream_index] = next_stream_offset;
        self.next_output_offset = next_output_offset;
        self.retain_replay(observed, limits);
        Ok(())
    }

    fn retain_replay(&mut self, output: ObservedOutput, limits: TerminalRegistryLimits) {
        self.replay_bytes = self.replay_bytes.saturating_add(output.bytes.len());
        self.replay.push_back(output);
        while self.replay.len() > limits.maximum_replay_events_per_process()
            || self.replay_bytes > limits.maximum_replay_bytes_per_process()
        {
            let Some(removed) = self.replay.pop_front() else { break };
            self.replay_bytes = self.replay_bytes.saturating_sub(removed.bytes.len());
            self.replay_complete = false;
        }
    }

    fn accept_terminal(
        &mut self,
        result: TerminalResult,
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if result.process_id() != self.process_id || result.plan_digest() != self.plan_digest {
            self.fail(
                TerminalBridgeErrorKind::ProcessIdentityMismatch,
                "terminal result identity does not match its live registration",
            );
            return Err(rejected(
                TerminalBridgeErrorKind::ProcessIdentityMismatch,
                "terminal result identity does not match its live registration",
            ));
        }
        let disposition = exit_disposition(&result);
        for attachment in self.attachments.values_mut() {
            attachment.enqueue_exit(disposition, limits)?;
        }
        self.terminal = Some(result);
        Ok(())
    }
}

const fn rejected(kind: TerminalBridgeErrorKind, detail: &'static str) -> TerminalBridgeError {
    TerminalBridgeError::rejected(kind, detail)
}

const fn stream_index(stream: OutputStream) -> usize {
    match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
        OutputStream::Terminal => 2,
    }
}

const fn terminal_stream(stream: OutputStream) -> TerminalStream {
    match stream {
        OutputStream::Stdout => TerminalStream::Stdout,
        OutputStream::Stderr => TerminalStream::Stderr,
        OutputStream::Terminal => TerminalStream::Terminal,
    }
}

const fn exit_disposition(result: &TerminalResult) -> TerminalExitDisposition {
    match result.os_exit() {
        OsExitObservation::Code(code) => TerminalExitDisposition::Code(*code),
        OsExitObservation::Signal(signal) if *signal > 0 => {
            TerminalExitDisposition::Signal(*signal)
        }
        OsExitObservation::Signal(_)
        | OsExitObservation::SignalName(_)
        | OsExitObservation::PlatformException(_)
        | OsExitObservation::Unavailable => TerminalExitDisposition::Unknown,
    }
}
