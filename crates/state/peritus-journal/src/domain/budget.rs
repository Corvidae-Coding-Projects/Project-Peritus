//! Commit-once budget transitions and durable non-activation observations.

use peritus_budget::{BudgetOperation, BudgetTransition};
use peritus_codec::{CodecLimits, encode_message};
use peritus_protocol::{BudgetReceiptDto, BudgetSnapshotDto, ReservationSnapshotDto};
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};

use crate::{
    AppendRequest, CommittedBatch, JournalError, JournalErrorKind, SqliteJournal, StateInstall,
    domain::{commit, encoding},
};

const NAMESPACE: u16 = 102;
const DOMAIN: &[u8] = b"peritus.budget-transition.v1";
const VALUE_KIND: u16 = 2;

/// Opaque C0 observation that a committed reservation lineage remains held and never activated.
pub struct NonActivationObservation {
    budget_id: BudgetId,
    reservation_id: BudgetReservationId,
    state_revision: u64,
    state_digest: Sha256Digest,
    producing_position: u64,
}

impl NonActivationObservation {
    /// Returns the observed reservation identity.
    #[must_use]
    pub const fn reservation_id(&self) -> BudgetReservationId {
        self.reservation_id
    }

    /// Returns the event position that committed the still-held lineage.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}

/// Move-only request binding one verified budget transition to a durable CAS state.
pub struct BudgetCommitRequest {
    append: AppendRequest,
    transition: BudgetTransition,
    install: StateInstall,
}

impl BudgetCommitRequest {
    /// Binds a logical transition to exact canonical receipt and successor snapshot frames.
    ///
    /// `CancelHeld` additionally requires the matching opaque current non-activation observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for mismatched lineage, missing non-activation proof, budget snapshot
    /// failure, canonical encoding failure, or state revision overflow.
    pub fn new(
        append: AppendRequest,
        transition: BudgetTransition,
        expected_revision: Option<u64>,
        non_activation: Option<&NonActivationObservation>,
    ) -> Result<Self, JournalError> {
        let receipt = transition.receipt();
        validate_non_activation(
            receipt.operation(),
            receipt.budget_id(),
            receipt.reservation_id(),
            expected_revision,
            non_activation,
        )?;
        let revision = commit::successor(expected_revision)?;
        let key = state_key(receipt.budget_id(), receipt.reservation_id());
        let value = encode_transition(&transition, non_activation)?;
        let install = StateInstall::new(NAMESPACE, key, expected_revision, revision, value)?;
        Ok(Self { append, transition, install })
    }
}

/// Opaque committed budget transition retaining the exact next ledger.
pub struct CommittedBudgetTransition {
    batch: CommittedBatch,
    transition: BudgetTransition,
    state_revision: u64,
    state_digest: Sha256Digest,
}

impl CommittedBudgetTransition {
    /// Borrows the exact committed event batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact logical successor transition.
    #[must_use]
    pub const fn transition(&self) -> &BudgetTransition {
        &self.transition
    }

    /// Returns the installed compare-and-swap revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the digest of the exact canonical state value.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Consumes the receipt into its committed batch and exact next ledger.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, peritus_budget::BudgetLedger) {
        (self.batch, self.transition.into_ledger())
    }
}

impl SqliteJournal {
    /// Commits one budget transition and its canonical successor projection exactly once.
    ///
    /// # Errors
    ///
    /// Returns journal CAS, idempotency, storage, or integrity failures.
    pub fn commit_budget_transition(
        &mut self,
        request: BudgetCommitRequest,
    ) -> Result<CommittedBudgetTransition, JournalError> {
        let BudgetCommitRequest { append, transition, install } = request;
        let (batch, state) = commit::commit_state(self, append, DOMAIN, install)?;
        Ok(CommittedBudgetTransition {
            batch,
            transition,
            state_revision: state.revision(),
            state_digest: state.digest(),
        })
    }

    /// Observes a current committed `Begin` lineage that is still held and has no activation.
    ///
    /// # Errors
    ///
    /// Returns not-found when the current reservation state is absent, activated, or terminal,
    /// and integrity failure when the stored canonical state value is malformed.
    pub fn observe_budget_non_activation(
        &self,
        reservation_id: BudgetReservationId,
    ) -> Result<NonActivationObservation, JournalError> {
        let state = self.state_record(NAMESPACE, reservation_id.as_bytes())?.ok_or_else(|| {
            JournalError::new(
                JournalErrorKind::NotFound,
                "observe budget non-activation",
                "reservation lineage is not durably current",
            )
        })?;
        let (operation, budget_id, stored_reservation) = decode_identity(state.bytes())?;
        if operation != BudgetOperation::Begin || stored_reservation != Some(reservation_id) {
            return Err(JournalError::new(
                JournalErrorKind::NotFound,
                "observe budget non-activation",
                "reservation is not in its committed held begin state",
            ));
        }
        Ok(NonActivationObservation {
            budget_id,
            reservation_id,
            state_revision: state.revision(),
            state_digest: state.digest(),
            producing_position: state.producing_position(),
        })
    }
}

fn encode_transition(
    transition: &BudgetTransition,
    predecessor: Option<&NonActivationObservation>,
) -> Result<Vec<u8>, JournalError> {
    let receipt = transition.receipt();
    let receipt_frame = encode_message(&BudgetReceiptDto::from(receipt), CodecLimits::PRODUCTION)
        .map_err(|_| input("canonical budget receipt encoding failed"))?;
    let account = transition
        .account_snapshot(receipt.budget_id())
        .map_err(|_| input("budget transition has no exact successor account"))?;
    let account_frame = encode_message(&BudgetSnapshotDto::from(account), CodecLimits::PRODUCTION)
        .map_err(|_| input("canonical budget snapshot encoding failed"))?;
    let reservation_frame = receipt
        .reservation_id()
        .map(|reservation_id| {
            transition
                .reservation_snapshot(reservation_id)
                .map_err(|_| input("budget transition has no exact successor reservation"))
                .and_then(|snapshot| {
                    encode_message(&ReservationSnapshotDto::from(snapshot), CodecLimits::PRODUCTION)
                        .map_err(|_| input("canonical reservation snapshot encoding failed"))
                })
        })
        .transpose()?;
    let mut payload = Vec::with_capacity(512);
    encoding::u16_value(&mut payload, operation_tag(receipt.operation()));
    payload.extend_from_slice(receipt.budget_id().as_bytes());
    match receipt.reservation_id() {
        Some(id) => {
            encoding::u8_value(&mut payload, 1);
            payload.extend_from_slice(id.as_bytes());
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    encoding::optional_digest(&mut payload, predecessor.map(|value| value.state_digest));
    encoding::bytes_value(&mut payload, &receipt_frame);
    encoding::bytes_value(&mut payload, &account_frame);
    match reservation_frame {
        Some(frame) => {
            encoding::u8_value(&mut payload, 1);
            encoding::bytes_value(&mut payload, &frame);
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    Ok(encoding::value(VALUE_KIND, &payload))
}

fn validate_non_activation(
    operation: BudgetOperation,
    budget_id: BudgetId,
    reservation_id: Option<BudgetReservationId>,
    expected_revision: Option<u64>,
    observation: Option<&NonActivationObservation>,
) -> Result<(), JournalError> {
    if operation == BudgetOperation::CancelHeld {
        let reservation_id =
            reservation_id.ok_or_else(|| input("CancelHeld has no reservation"))?;
        let observation = observation
            .ok_or_else(|| input("CancelHeld requires a current C0 non-activation observation"))?;
        if observation.budget_id != budget_id
            || observation.reservation_id != reservation_id
            || expected_revision != Some(observation.state_revision)
        {
            return Err(input("non-activation observation names another current lineage"));
        }
    } else if observation.is_some() {
        return Err(input("non-activation observation is valid only for CancelHeld"));
    }
    Ok(())
}

fn decode_identity(
    value: &[u8],
) -> Result<(BudgetOperation, BudgetId, Option<BudgetReservationId>), JournalError> {
    let payload = encoding::payload(value, VALUE_KIND)
        .ok_or_else(|| corrupt("budget state header is malformed"))?;
    if payload.len() < 19 {
        return Err(corrupt("budget state identity is truncated"));
    }
    let operation = operation_from_tag(u16::from_be_bytes([payload[0], payload[1]]))?;
    let budget_id = BudgetId::new(
        payload[2..18].try_into().map_err(|_| corrupt("budget identity length is invalid"))?,
    )
    .map_err(|_| corrupt("stored budget identity is invalid"))?;
    let reservation = match payload[18] {
        0 => None,
        1 if payload.len() >= 35 => Some(
            BudgetReservationId::new(
                payload[19..35]
                    .try_into()
                    .map_err(|_| corrupt("reservation identity length is invalid"))?,
            )
            .map_err(|_| corrupt("stored reservation identity is invalid"))?,
        ),
        _ => return Err(corrupt("stored reservation option is malformed")),
    };
    Ok((operation, budget_id, reservation))
}

fn state_key(budget_id: BudgetId, reservation_id: Option<BudgetReservationId>) -> Vec<u8> {
    reservation_id.map_or_else(|| budget_id.as_bytes().to_vec(), |id| id.as_bytes().to_vec())
}

const fn operation_tag(operation: BudgetOperation) -> u16 {
    match operation {
        BudgetOperation::AllocateChild => 1,
        BudgetOperation::Begin => 2,
        BudgetOperation::Activate => 3,
        BudgetOperation::ObserveUsage => 4,
        BudgetOperation::SettleExact => 5,
        BudgetOperation::CancelHeld => 6,
        BudgetOperation::FinalizeAmbiguous => 7,
        BudgetOperation::Seal => 8,
        BudgetOperation::Close => 9,
    }
}

const fn operation_from_tag(tag: u16) -> Result<BudgetOperation, JournalError> {
    match tag {
        1 => Ok(BudgetOperation::AllocateChild),
        2 => Ok(BudgetOperation::Begin),
        3 => Ok(BudgetOperation::Activate),
        4 => Ok(BudgetOperation::ObserveUsage),
        5 => Ok(BudgetOperation::SettleExact),
        6 => Ok(BudgetOperation::CancelHeld),
        7 => Ok(BudgetOperation::FinalizeAmbiguous),
        8 => Ok(BudgetOperation::Seal),
        9 => Ok(BudgetOperation::Close),
        _ => Err(corrupt("stored budget operation is unknown")),
    }
}

const fn input(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "plan budget commit", detail)
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "read budget state", detail)
}
