//! B1 budget wire contract tests.

use peritus_budget::{
    Activation, AmbiguousFinalization, BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits,
    BudgetRequest, ChildBudgetRequest, ReservationReference, UsageFinality, UsageObservation,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CodecLimits, decode_message, encode_message,
};
use peritus_protocol::{
    BudgetAmountsDto, BudgetCommandDto, BudgetErrorDto, BudgetReceiptDto, BudgetSnapshotDto,
    ReservationSnapshotDto,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, BudgetId, BudgetReservationId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;

fn fixture_id<T, E: core::fmt::Debug>(
    byte: u8,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> T {
    constructor([byte; 16]).expect("nonzero fixture id")
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        fixture_id(1, AcceptanceSpecId::new),
        fixture_id(2, HarnessId::new),
        fixture_id(3, WorkspaceId::new),
        Generation::new(4).expect("one based"),
        RevisionNumber::new(5).expect("one based"),
        fixture_id(6, PolicyId::new),
        fixture_id(7, ProviderProfileId::new),
    )
}

fn roundtrip<T>(value: &T) -> T
where
    T: CanonicalEncode + CanonicalDecode,
{
    decode_message(&encode_message(value, LIMITS).expect("encode"), LIMITS).expect("decode")
}

fn request() -> BudgetRequest {
    BudgetRequest::new(
        fixture_id(20, BudgetReservationId::new),
        fixture_id(21, BudgetId::new),
        revision(),
        fixture_id(22, ActionId::new),
        Sha256Digest::new([23; 32]),
        BudgetAmounts::from_units(1, 2, 3, 1, 0),
        BudgetAmounts::from_units(10, 20, 30, 0, 0),
    )
}

fn reference() -> ReservationReference {
    ReservationReference::new(
        request().reservation_id(),
        request().action_id(),
        request().action_digest(),
        Sha256Digest::new([24; 32]),
    )
}

#[test]
fn amounts_and_every_budget_command_roundtrip() {
    let amounts = BudgetAmounts::from_units(100, 200, 300, 4, 5);
    assert_eq!(roundtrip(&BudgetAmountsDto::from(amounts)).into_domain(), amounts);
    let reference = reference();
    let commands = [
        BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            fixture_id(30, BudgetId::new),
            fixture_id(31, BudgetId::new),
            revision(),
            BudgetLimits::new(amounts),
        )),
        BudgetCommand::Begin(request()),
        BudgetCommand::Activate(Activation::new(
            reference.reservation_id(),
            reference.action_id(),
            reference.action_digest(),
            reference.evidence_digest(),
        )),
        BudgetCommand::ObserveUsage(UsageObservation::new(
            reference.reservation_id(),
            reference.action_id(),
            reference.action_digest(),
            reference.evidence_digest(),
            amounts,
            UsageFinality::Final,
        )),
        BudgetCommand::SettleExact(reference),
        BudgetCommand::CancelHeld(reference),
        BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(reference)),
        BudgetCommand::Seal(fixture_id(32, BudgetId::new)),
        BudgetCommand::Close(fixture_id(33, BudgetId::new)),
    ];
    for command in commands {
        assert_eq!(roundtrip(&BudgetCommandDto::from(command)).into_domain(), command);
    }
}

#[test]
fn snapshots_receipts_and_failures_remain_inert_data() {
    let root_id = request().budget_id();
    let limits = BudgetLimits::new(BudgetAmounts::from_units(1_000, 2_000, 3_000, 10, 10));
    let ledger = BudgetLedger::new_root(root_id, revision(), limits);
    let account = BudgetSnapshotDto::from(ledger.account(root_id).expect("root snapshot"));
    assert_eq!(roundtrip(&account), account);

    let transition = ledger.transition(BudgetCommand::Begin(request())).expect("begin");
    let receipt = BudgetReceiptDto::from(transition.receipt());
    assert_eq!(roundtrip(&receipt), receipt);
    let reservation = ReservationSnapshotDto::from(
        transition.ledger().reservation(request().reservation_id()).expect("reservation"),
    );
    assert_eq!(roundtrip(&reservation), reservation);

    let error = BudgetErrorDto::from(
        transition.ledger().account(fixture_id(99, BudgetId::new)).expect_err("unknown account"),
    );
    assert_eq!(roundtrip(&error), error);
}
