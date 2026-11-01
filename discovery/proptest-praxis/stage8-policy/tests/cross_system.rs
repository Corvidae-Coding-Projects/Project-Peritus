use peritus_gates::{GateExecutionRecord, TargetGatePlan, TargetGateReport};
use peritus_obligations::{
    DirectEvidence, EvidenceBinding, ObligationLimits, ObligationSpec, PublicTaskSource,
    RequirementDraft, RequirementEvidence, RequirementLedger, qualify,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, SettlementCause, SettlementReducer,
};
use peritus_spec::RequirementId;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use std::path::PathBuf;

fn digest(tag: u8) -> Sha256Digest {
    Sha256Digest::new([tag; 32])
}

fn candidate() -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        digest(3),
        digest(4),
        Some(digest(5)),
        1,
        1,
    )
    .unwrap()
}

fn ledger() -> RequirementLedger {
    let limits = ObligationLimits::production();
    let source = PublicTaskSource::new(b"Produce a working file.".to_vec(), 1, limits).unwrap();
    RequirementLedger::extract(
        &source,
        vec![RequirementDraft::new(
            RequirementId::new(digest(6)),
            0,
            source.content().len(),
            ObligationSpec::Hard,
            vec![],
        )],
        limits,
    )
    .unwrap()
}

fn gate_report(pass: bool) -> TargetGateReport {
    let root = std::env::temp_dir().join(format!(
        "peritus-stage8-cross-system-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("alpha.py"), b"print('hello')\n").unwrap();
    let plan = TargetGatePlan::discover(&root, vec![PathBuf::from("alpha.py")], &[]).unwrap();
    let mut records: Vec<_> = plan
        .commands()
        .iter()
        .map(|command| GateExecutionRecord {
            command: command.display(),
            label: command.label().to_owned(),
            exit_code: Some(0),
            output: String::new(),
        })
        .collect();
    if !pass {
        records[0].exit_code = Some(1);
    }
    let report = TargetGateReport::from_execution(&plan, records);
    std::fs::remove_file(root.join("alpha.py")).unwrap();
    std::fs::remove_dir(root).unwrap();
    report
}

fn status(
    candidate: CandidateIdentity,
    good: bool,
    dependencies: EvidenceDependencies,
) -> EvidenceStatus<QualificationEvidence> {
    let value =
        if good { QualificationEvidence::Satisfied } else { QualificationEvidence::Unsatisfied };
    let record = EvidenceRecord::new(candidate, dependencies, value);
    if good { EvidenceStatus::Current(record) } else { EvidenceStatus::Failed(record) }
}

fn check(gate_pass: bool, requirement_state: u8, review_pass: bool) {
    let candidate = candidate();
    let ledger = ledger();
    let mut evidence = Vec::new();
    if requirement_state != 0 {
        let binding = EvidenceBinding::new(
            ledger.entries()[0].id(),
            if requirement_state == 3 { digest(99) } else { ledger.digest() },
            candidate,
            digest(10),
            vec![],
            ledger.limits(),
        )
        .unwrap();
        evidence.push(RequirementEvidence::Direct(DirectEvidence::new(
            binding,
            requirement_state == 1 || requirement_state == 3,
        )));
    }
    let obligation_report = qualify(&ledger, &candidate, &[], &evidence).unwrap();
    assert_eq!(obligation_report.qualified(), requirement_state == 1);
    let gates = gate_report(gate_pass);
    assert_eq!(gates.passed(), gate_pass);
    let can_accept = gate_pass && obligation_report.qualified() && review_pass;
    let gate_status = status(candidate, gates.passed(), EvidenceDependencies::GATES);
    let obligation_status =
        status(candidate, obligation_report.qualified(), EvidenceDependencies::OBLIGATIONS);
    let review_status = status(candidate, review_pass, EvidenceDependencies::REVIEW);
    let qualified = CandidateCheckpoint::new(
        candidate,
        CandidateStage::Qualified,
        gate_status,
        obligation_status,
        review_status,
    );
    assert_eq!(
        qualified.is_ok(),
        can_accept,
        "gates={gate_pass} obligation={requirement_state} review={review_pass}"
    );
    let checkpoint = qualified.unwrap_or_else(|_| {
        CandidateCheckpoint::new(
            candidate,
            CandidateStage::Changed,
            gate_status,
            obligation_status,
            review_status,
        )
        .unwrap()
    });
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).unwrap();
    let result = reducer.settle(SettlementCause::Completed).unwrap();
    assert_eq!(result.is_accepted(), can_accept);
    assert!(reducer.settle(SettlementCause::Completed).is_err());
}

#[test]
fn fixed_cross_system_truth_table() {
    for gate in [false, true] {
        for requirement in 0..=3 {
            for review in [false, true] {
                check(gate, requirement, review);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), .. ProptestConfig::default() })]
    #[test]
    fn generated_cross_system_outcomes(gate in any::<bool>(), requirement in 0u8..4,
        review in any::<bool>()) {
        check(gate, requirement, review);
    }
}
