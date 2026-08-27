//! Checked application-ledger values.

use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, Sha256Digest, WorkspaceId};

use crate::{CommittedBatch, JournalError, JournalErrorKind};

/// Maximum number of workspace registrations returned by one recovery page.
pub const MAX_APPLICATION_WORKSPACE_PAGE: usize = 4_096;

/// Maximum canonical bytes retained for one application workspace registration.
pub const MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES: usize = 1_048_576;

/// Opaque nonzero application request identity retained in the command ledger.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApplicationRequestId([u8; 16]);

impl ApplicationRequestId {
    /// Creates an application request identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input when all bytes are zero.
    pub fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if bytes == [0; 16] {
            Err(invalid("application request identity must be nonzero"))
        } else {
            Ok(Self(bytes))
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Operating-system principal namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPrincipalKind {
    /// Unix-domain peer credentials.
    UnixPeer,
    /// Windows named-pipe peer credentials.
    WindowsPeer,
    /// Locally provisioned service identity.
    Service,
}

impl ApplicationPrincipalKind {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::UnixPeer => 1,
            Self::WindowsPeer => 2,
            Self::Service => 3,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::UnixPeer),
            2 => Some(Self::WindowsPeer),
            3 => Some(Self::Service),
            _ => None,
        }
    }
}

/// Durable principal binding state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPrincipalState {
    /// The binding may authenticate new connections.
    Active,
    /// The binding has been explicitly revoked.
    Revoked,
}

impl ApplicationPrincipalState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Active),
            2 => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// New immutable OS-principal to actor binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationPrincipal {
    pub(super) principal_digest: Sha256Digest,
    pub(super) kind: ApplicationPrincipalKind,
    pub(super) actor_id: ActorId,
    pub(super) binding_digest: Sha256Digest,
}

impl NewApplicationPrincipal {
    /// Creates a checked principal binding request.
    #[must_use]
    pub const fn new(
        principal_digest: Sha256Digest,
        kind: ApplicationPrincipalKind,
        actor_id: ActorId,
        binding_digest: Sha256Digest,
    ) -> Self {
        Self { principal_digest, kind, actor_id, binding_digest }
    }
}

/// One durable OS-principal to human actor binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationPrincipal {
    pub(super) principal_digest: Sha256Digest,
    pub(super) kind: ApplicationPrincipalKind,
    pub(super) actor_id: ActorId,
    pub(super) binding_digest: Sha256Digest,
    pub(super) state: ApplicationPrincipalState,
}

impl ApplicationPrincipal {
    /// Returns the canonical principal digest.
    #[must_use]
    pub const fn principal_digest(&self) -> Sha256Digest {
        self.principal_digest
    }
    /// Returns the peer namespace.
    #[must_use]
    pub const fn kind(&self) -> ApplicationPrincipalKind {
        self.kind
    }
    /// Returns the uniquely bound human actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the provisioned binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    /// Returns current binding state.
    #[must_use]
    pub const fn state(&self) -> ApplicationPrincipalState {
        self.state
    }
}

/// Durable application session state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationSessionState {
    /// New requests may be admitted.
    Active,
    /// Existing work may drain but new mutations are rejected.
    Draining,
    /// The session is terminally closed.
    Closed,
}

impl ApplicationSessionState {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::Active => 1,
            Self::Draining => 2,
            Self::Closed => 3,
        }
    }
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Active),
            2 => Some(Self::Draining),
            3 => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Checked new durable session facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationSession {
    pub(super) session_id: SessionId,
    pub(super) actor_id: ActorId,
    pub(super) authority_epoch: u64,
    pub(super) created_at: u64,
    pub(super) protocol_id: [u8; 16],
    pub(super) version_major: u16,
    pub(super) version_minor: u16,
}

impl NewApplicationSession {
    /// Creates checked session facts.
    ///
    /// # Errors
    ///
    /// Returns invalid input for zero epoch, timestamp, protocol, or major version.
    #[allow(clippy::too_many_arguments, reason = "durable negotiated session facts are explicit")]
    pub fn new(
        session_id: SessionId,
        actor_id: ActorId,
        authority_epoch: u64,
        created_at: u64,
        protocol_id: [u8; 16],
        version_major: u16,
        version_minor: u16,
    ) -> Result<Self, JournalError> {
        if authority_epoch == 0 || created_at == 0 || protocol_id == [0; 16] || version_major == 0 {
            return Err(invalid("application session contains a zero required value"));
        }
        Ok(Self {
            session_id,
            actor_id,
            authority_epoch,
            created_at,
            protocol_id,
            version_major,
            version_minor,
        })
    }
}

/// One durable negotiated session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationSession {
    pub(super) session_id: SessionId,
    pub(super) actor_id: ActorId,
    pub(super) authority_epoch: u64,
    pub(super) state: ApplicationSessionState,
    pub(super) created_at: u64,
    pub(super) protocol_id: [u8; 16],
    pub(super) version_major: u16,
    pub(super) version_minor: u16,
}

impl ApplicationSession {
    /// Returns the durable session identity.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Returns the authenticated actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the authority epoch captured at admission.
    #[must_use]
    pub const fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }
    /// Returns session lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationSessionState {
        self.state
    }
    /// Returns the monotonic creation tick.
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    /// Returns the negotiated protocol identifier.
    #[must_use]
    pub const fn protocol_id(&self) -> &[u8; 16] {
        &self.protocol_id
    }
    /// Returns the negotiated major version.
    #[must_use]
    pub const fn version_major(&self) -> u16 {
        self.version_major
    }
    /// Returns the negotiated minor version.
    #[must_use]
    pub const fn version_minor(&self) -> u16 {
        self.version_minor
    }
}

/// Durable command lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationCommandState {
    /// Admitted before domain dispatch.
    Pending,
    /// The append acknowledgement was ambiguous and must be reconciled.
    Indeterminate,
    /// The exact committed range and response are retained.
    Committed,
    /// A stable pre-append rejection and response are retained.
    Rejected,
}

impl ApplicationCommandState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Pending),
            2 => Some(Self::Indeterminate),
            3 => Some(Self::Committed),
            4 => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// New actor/session/key command admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationCommand {
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) idempotency_key: Vec<u8>,
    pub(super) request_digest: Sha256Digest,
    pub(super) domain_command_digest: Sha256Digest,
    pub(super) request_id: ApplicationRequestId,
    pub(super) command_id: CommandId,
}

impl NewApplicationCommand {
    /// Creates a bounded ledger admission.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the key contains 1 through 256 bytes.
    pub fn new(
        actor_id: ActorId,
        session_id: SessionId,
        idempotency_key: Vec<u8>,
        request_digest: Sha256Digest,
        domain_command_digest: Sha256Digest,
        request_id: ApplicationRequestId,
        command_id: CommandId,
    ) -> Result<Self, JournalError> {
        if idempotency_key.is_empty() || idempotency_key.len() > 256 {
            return Err(invalid("application idempotency key is outside the production bound"));
        }
        Ok(Self {
            actor_id,
            session_id,
            idempotency_key,
            request_digest,
            domain_command_digest,
            request_id,
            command_id,
        })
    }
}

/// One durable application command ledger row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCommandRecord {
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) idempotency_key: Vec<u8>,
    pub(super) request_digest: Sha256Digest,
    pub(super) domain_command_digest: Sha256Digest,
    pub(super) request_id: ApplicationRequestId,
    pub(super) command_id: CommandId,
    pub(super) state: ApplicationCommandState,
    pub(super) first_position: Option<u64>,
    pub(super) last_position: Option<u64>,
    pub(super) error_code: Option<String>,
    pub(super) result_digest: Option<Sha256Digest>,
}

impl ApplicationCommandRecord {
    /// Returns the actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the session identity.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Borrows the exact idempotency key.
    #[must_use]
    pub fn idempotency_key(&self) -> &[u8] {
        &self.idempotency_key
    }
    /// Returns the complete application request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Returns SHA-256 over the exact canonical B3 domain-command frame.
    #[must_use]
    pub const fn domain_command_digest(&self) -> Sha256Digest {
        self.domain_command_digest
    }
    /// Returns the original application request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApplicationRequestId {
        self.request_id
    }
    /// Returns the reserved B3 command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns command lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationCommandState {
        self.state
    }
    /// Returns the first exact committed position when committed.
    #[must_use]
    pub const fn first_position(&self) -> Option<u64> {
        self.first_position
    }
    /// Returns the last exact committed position when committed.
    #[must_use]
    pub const fn last_position(&self) -> Option<u64> {
        self.last_position
    }
    /// Borrows the stable rejection code when rejected.
    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }
    /// Returns the exact final response-facts digest when terminal.
    #[must_use]
    pub const fn result_digest(&self) -> Option<Sha256Digest> {
        self.result_digest
    }
}

/// Result of durable idempotency admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCommandAdmission {
    /// A new pending row was durably inserted.
    Inserted(ApplicationCommandRecord),
    /// The key and exact request digest already exist.
    Existing(ApplicationCommandRecord),
    /// The actor/session/key tuple is bound to a different digest.
    Conflict(ApplicationCommandRecord),
}

/// Terminal or recoverable settlement applied to an admitted command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCommandSettlement {
    pub(super) kind: SettlementKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SettlementKind {
    Indeterminate,
    Committed { first_position: u64, last_position: u64, result_digest: Sha256Digest },
    Rejected { error_code: String, result_digest: Sha256Digest },
}

impl ApplicationCommandSettlement {
    /// Creates a recovery-required indeterminate settlement.
    #[must_use]
    pub const fn indeterminate() -> Self {
        Self { kind: SettlementKind::Indeterminate }
    }

    /// Creates a committed settlement from a real checked C0 commit receipt.
    #[must_use]
    pub const fn committed(batch: &CommittedBatch, result_digest: Sha256Digest) -> Self {
        Self {
            kind: SettlementKind::Committed {
                first_position: batch.first_position(),
                last_position: batch.last_position(),
                result_digest,
            },
        }
    }

    /// Creates a checked stable rejection settlement.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the stable code contains 1 through 128 ASCII graphic bytes.
    pub fn rejected(error_code: String, result_digest: Sha256Digest) -> Result<Self, JournalError> {
        if error_code.is_empty()
            || error_code.len() > 128
            || !error_code.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(invalid("application command rejection code is invalid"));
        }
        Ok(Self { kind: SettlementKind::Rejected { error_code, result_digest } })
    }
}

/// Application artifact catalog state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationArtifactState {
    /// Upload metadata exists but content is not yet published.
    Uploading,
    /// Immutable content and its producing journal position are available.
    Available,
    /// The attempted upload failed and may be inspected or replaced explicitly.
    Failed,
}

impl ApplicationArtifactState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Uploading),
            2 => Some(Self::Available),
            3 => Some(Self::Failed),
            _ => None,
        }
    }
}

/// New bounded artifact catalog row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationArtifact {
    pub(super) artifact_id: ArtifactId,
    pub(super) digest: Sha256Digest,
    pub(super) byte_size: u64,
    pub(super) media_type: String,
}

impl NewApplicationArtifact {
    /// Creates pending artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an empty, oversized, or non-ASCII media type.
    pub fn new(
        artifact_id: ArtifactId,
        digest: Sha256Digest,
        byte_size: u64,
        media_type: String,
    ) -> Result<Self, JournalError> {
        if media_type.is_empty() || media_type.len() > 255 || !media_type.is_ascii() {
            return Err(invalid("application artifact media type is invalid"));
        }
        if byte_size > i64::MAX as u64 {
            return Err(invalid("application artifact size exceeds SQLite range"));
        }
        Ok(Self { artifact_id, digest, byte_size, media_type })
    }
}

/// One durable application artifact catalog row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationArtifact {
    pub(super) artifact_id: ArtifactId,
    pub(super) digest: Sha256Digest,
    pub(super) byte_size: u64,
    pub(super) media_type: String,
    pub(super) state: ApplicationArtifactState,
    pub(super) producing_position: Option<u64>,
}

impl ApplicationArtifact {
    /// Returns the stable artifact identity.
    #[must_use]
    pub const fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }
    /// Returns the immutable content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns exact byte length.
    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }
    /// Borrows the media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    /// Returns catalog lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationArtifactState {
        self.state
    }
    /// Returns the producing journal position once available.
    #[must_use]
    pub const fn producing_position(&self) -> Option<u64> {
        self.producing_position
    }
}

/// Application workspace registration state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationWorkspaceState {
    /// The workspace is registered and may admit work.
    Registered,
    /// Registration is retained but the workspace is temporarily unavailable.
    Unavailable,
    /// Registration history is retained but new work is forbidden.
    Removed,
}

impl ApplicationWorkspaceState {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::Registered => 1,
            Self::Unavailable => 2,
            Self::Removed => 3,
        }
    }
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Registered),
            2 => Some(Self::Unavailable),
            3 => Some(Self::Removed),
            _ => None,
        }
    }
}

/// New exact workspace registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationWorkspace {
    pub(super) workspace_id: WorkspaceId,
    pub(super) registration_bytes: Vec<u8>,
    pub(super) registration_digest: Sha256Digest,
}

impl NewApplicationWorkspace {
    /// Creates a bounded exact registration.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the registration contains 1 byte through 1 MiB.
    pub fn new(
        workspace_id: WorkspaceId,
        registration_bytes: Vec<u8>,
        registration_digest: Sha256Digest,
    ) -> Result<Self, JournalError> {
        if registration_bytes.is_empty()
            || registration_bytes.len() > MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES
        {
            return Err(invalid(
                "application workspace registration is outside the production bound",
            ));
        }
        if peritus_codec::sha256(&registration_bytes) != registration_digest {
            return Err(invalid(
                "application workspace registration digest differs from its bytes",
            ));
        }
        Ok(Self { workspace_id, registration_bytes, registration_digest })
    }
}

/// One durable exact workspace registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationWorkspace {
    pub(super) workspace_id: WorkspaceId,
    pub(super) registration_bytes: Vec<u8>,
    pub(super) registration_digest: Sha256Digest,
    pub(super) state: ApplicationWorkspaceState,
}

impl ApplicationWorkspace {
    /// Returns the workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Borrows exact retained registration bytes.
    #[must_use]
    pub fn registration_bytes(&self) -> &[u8] {
        &self.registration_bytes
    }
    /// Returns the exact registration digest.
    #[must_use]
    pub const fn registration_digest(&self) -> Sha256Digest {
        self.registration_digest
    }
    /// Returns workspace lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationWorkspaceState {
        self.state
    }
}

/// One bounded deterministic page of durable workspace registrations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationWorkspacePage {
    pub(super) workspaces: Vec<ApplicationWorkspace>,
    pub(super) next_after: Option<WorkspaceId>,
}

impl ApplicationWorkspacePage {
    /// Borrows workspace registrations in strict identity order.
    #[must_use]
    pub fn workspaces(&self) -> &[ApplicationWorkspace] {
        &self.workspaces
    }

    /// Returns the exclusive identity cursor for the next page, when more rows exist.
    #[must_use]
    pub const fn next_after(&self) -> Option<WorkspaceId> {
        self.next_after
    }

    /// Consumes the page and returns its ordered registrations.
    #[must_use]
    pub fn into_workspaces(self) -> Vec<ApplicationWorkspace> {
        self.workspaces
    }
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application ledger value", detail)
}
