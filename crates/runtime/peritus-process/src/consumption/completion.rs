//! Durable independently-owned process completion receipts.

use std::sync::Arc;

use peritus_types::Sha256Digest;

use crate::{
    ErrorCode, OutputStream, ProcessControl, ProcessCursor, ProcessError, ProcessOperation,
    ProcessTreeIdentity, RecoveryClass, RetainedOwnerObservation, RetainedOwnerRequest,
    RetainedProcessKey, RetainedServiceOwner, RetainedStreamPage, TerminalResult,
    registry_storage,
};

use super::{ProcessStore, store_error};

/// Canonical identity bound into one service-generation completion receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedCompletionBinding {
    service_owner: RetainedServiceOwner,
    key: RetainedProcessKey,
    request_digest: Sha256Digest,
    plan_digest: Sha256Digest,
}

impl RetainedCompletionBinding {
    /// Binds a completion to the exact service generation, request, and checked plan.
    #[must_use]
    pub const fn new(
        service_owner: RetainedServiceOwner,
        key: RetainedProcessKey,
        request_digest: Sha256Digest,
        plan_digest: Sha256Digest,
    ) -> Self {
        Self { service_owner, key, request_digest, plan_digest }
    }

    /// Returns the authenticated service generation.
    #[must_use]
    pub const fn service_owner(self) -> RetainedServiceOwner { self.service_owner }
    /// Returns the exact retained process identity.
    #[must_use]
    pub const fn key(self) -> RetainedProcessKey { self.key }
    /// Returns the digest of the complete retained owner request.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest { self.request_digest }
    /// Returns the exact checked execution-plan digest.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest { self.plan_digest }
}

/// One stable retained-stream snapshot that may remain outstanding across completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedStreamSnapshot {
    stream: OutputStream,
    digest: Sha256Digest,
    bytes: Arc<[u8]>,
}

impl RetainedStreamSnapshot {
    /// Freezes the exact bytes and their content digest.
    #[must_use]
    pub fn new(stream: OutputStream, bytes: Vec<u8>) -> Self {
        let digest = peritus_codec::sha256(&bytes);
        Self { stream, digest, bytes: bytes.into() }
    }

    /// Returns the represented stream.
    #[must_use]
    pub const fn stream(&self) -> OutputStream { self.stream }
    /// Returns the digest of the complete snapshot.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Returns the complete stable snapshot bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
}

/// Small authenticated completion header used by late status and control calls.
#[derive(Clone, Debug)]
pub struct RetainedOwnerCompletionStatus {
    binding: RetainedCompletionBinding,
    terminal: TerminalResult,
    tree: Option<ProcessTreeIdentity>,
}

impl RetainedOwnerCompletionStatus {
    pub(crate) const fn new(
        binding: RetainedCompletionBinding,
        terminal: TerminalResult,
        tree: Option<ProcessTreeIdentity>,
    ) -> Self {
        Self { binding, terminal, tree }
    }

    /// Returns the exact receipt binding.
    #[must_use]
    pub const fn binding(&self) -> RetainedCompletionBinding { self.binding }
    /// Returns the unique durable terminal result.
    #[must_use]
    pub const fn terminal_result(&self) -> &TerminalResult { &self.terminal }
    /// Returns the native process tree selected by the original owner.
    #[must_use]
    pub const fn tree_identity(&self) -> Option<ProcessTreeIdentity> { self.tree }
}

impl ProcessStore {
    /// Atomically publishes a complete retained-owner diagnostic receipt.
    ///
    /// Event history is transferred in physical pages and streams in bounded chunks. The
    /// published directory becomes visible only after every component is synchronized.
    ///
    /// # Errors
    /// Returns a typed persistence or binding failure. A failure leaves the live owner control
    /// and supplied outstanding snapshots with the caller for an exact retry.
    pub fn persist_retained_owner_completion(
        &self,
        binding: RetainedCompletionBinding,
        terminal: &TerminalResult,
        control: Option<&ProcessControl>,
        outstanding: &[Option<RetainedStreamSnapshot>; 3],
    ) -> Result<(), ProcessError> {
        self.validate_completion_authority(binding, terminal, control, outstanding)?;
        registry_storage::persist_owner_completion(
            &self.inner.completion_receipts,
            &self.inner.retained_owners,
            binding,
            terminal,
            control,
            outstanding,
        )
    }

    /// Loads the small exact completion header without loading event or stream diagnostics.
    ///
    /// # Errors
    /// Rejects a receipt for another service generation, key, or request digest.
    pub fn retained_owner_completion_status(
        &self,
        key: RetainedProcessKey,
        service_owner: RetainedServiceOwner,
        request_digest: Option<Sha256Digest>,
    ) -> Result<Option<RetainedOwnerCompletionStatus>, ProcessError> {
        registry_storage::load_owner_completion_status(
            &self.inner.completion_receipts,
            key,
            service_owner,
            request_digest,
        )
    }

    /// Loads only the event pages selected by one late observation cursor.
    ///
    /// # Errors
    /// Returns a typed corruption or identity error for a noncanonical receipt.
    pub fn retained_owner_completion_observe(
        &self,
        key: RetainedProcessKey,
        service_owner: RetainedServiceOwner,
        cursor: ProcessCursor,
        max_events: usize,
    ) -> Result<Option<RetainedOwnerObservation>, ProcessError> {
        registry_storage::load_owner_completion_observation(
            &self.inner.completion_receipts,
            key,
            service_owner,
            cursor,
            max_events,
        )
    }

    /// Loads one bounded page from the final or still-outstanding stable stream snapshot.
    ///
    /// # Errors
    /// Returns a typed corruption, identity, digest, or offset error.
    pub fn retained_owner_completion_stream_page(
        &self,
        key: RetainedProcessKey,
        service_owner: RetainedServiceOwner,
        stream: OutputStream,
        snapshot_digest: Option<Sha256Digest>,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Option<RetainedStreamPage>, ProcessError> {
        registry_storage::load_owner_completion_stream_page(
            &self.inner.completion_receipts,
            key,
            service_owner,
            stream,
            snapshot_digest,
            offset,
            max_bytes,
        )
    }

    fn validate_completion_authority(
        &self,
        binding: RetainedCompletionBinding,
        terminal: &TerminalResult,
        control: Option<&ProcessControl>,
        outstanding: &[Option<RetainedStreamSnapshot>; 3],
    ) -> Result<(), ProcessError> {
        let key = binding.key();
        let record = self
            .authoritative_record(key.process_id())?
            .ok_or_else(|| store_error("completion receipt has no consumed authority"))?;
        let claim = record
            .claim
            .ok_or_else(|| store_error("completion receipt has no consumption claim"))?;
        let owner = claim
            .retained_owner()
            .ok_or_else(|| completion_mismatch("completion claim has no retained owner"))?;
        let manifest = record
            .manifest
            .ok_or_else(|| store_error("completion receipt has no terminal manifest"))?;
        if claim.process_id() != key.process_id()
            || claim.plan_digest() != binding.plan_digest()
            || owner.operation_digest() != key.operation_digest()
            || owner.request_digest() != binding.request_digest()
            || !claim.matches_manifest(&manifest)
            || manifest.terminal.as_ref() != Some(terminal)
            || terminal.process_id() != key.process_id()
            || terminal.plan_digest() != binding.plan_digest()
        {
            return Err(completion_mismatch("completion authority or terminal binding differs"));
        }
        let request_bytes = registry_storage::load_owner_completion_request(
            &self.inner.retained_owners,
            key.process_id(),
        )?;
        let request = RetainedOwnerRequest::decode(request_bytes)?;
        if request.digest() != binding.request_digest()
            || request.binding().service_owner() != binding.service_owner()
            || RetainedProcessKey::from_binding(request.binding()) != key
            || request.execution_plan().digest() != binding.plan_digest()
        {
            return Err(completion_mismatch("completion request binding differs"));
        }
        match control {
            Some(control)
                if control.terminal_result().as_ref() != Some(terminal)
                    || control.tree_identity() != manifest.tree =>
            {
                return Err(completion_mismatch(
                    "completion control binding differs from durable terminal state",
                ));
            }
            None
                if manifest.tree.is_some()
                    || outstanding.iter().any(Option::is_some)
                    || terminal.output().streams().iter().any(|stream| stream.retained() != 0) =>
            {
                return Err(completion_mismatch(
                    "completion without a process control retains native evidence",
                ));
            }
            Some(_) | None => {}
        }
        for (index, snapshot) in outstanding.iter().enumerate() {
            if snapshot.as_ref().is_some_and(|snapshot| {
                stream_index(snapshot.stream()) != index
                    || peritus_codec::sha256(snapshot.bytes()) != snapshot.digest()
            }) {
                return Err(completion_mismatch("outstanding stream snapshot binding differs"));
            }
        }
        Ok(())
    }
}

const fn stream_index(stream: OutputStream) -> usize {
    match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
        OutputStream::Terminal => 2,
    }
}

const fn completion_mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::AuthorizationMismatch,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
