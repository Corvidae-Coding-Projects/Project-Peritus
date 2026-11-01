use peritus_gates::{GateExecutionRecord, GateObservation, TargetGatePlan, TargetGateReport};
use proptest::prelude::*;
use std::path::PathBuf;

fn check(statuses: &[u8], optional_mode: bool, extra_record: bool, constraint_passes: bool) {
    let root = std::env::temp_dir().join(format!(
        "peritus-stage8-gate-report-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("alpha.py"), b"print('hello')\n").unwrap();
    let discovered = TargetGatePlan::discover(&root, vec![PathBuf::from("alpha.py")], &[]).unwrap();
    assert!(discovered.has_complete_coverage());
    let plan =
        if optional_mode { discovered.with_conventional_checks_unselected() } else { discovered };
    let mut records = Vec::new();
    let mut observations = Vec::new();
    let mut expected = constraint_passes;
    for (index, command) in plan.commands().iter().enumerate() {
        let status = statuses.get(index).copied().unwrap_or(0) % 6;
        let record = |exit_code| GateExecutionRecord {
            command: command.display(),
            label: command.label().to_owned(),
            exit_code,
            output: String::new(),
        };
        match status {
            0 => expected = false, // missing
            1 => records.push(record(Some(0))),
            2 => {
                records.push(record(Some(1)));
                expected = false;
            }
            3 => {
                observations.push(GateObservation {
                    command: command.display(),
                    label: command.label().to_owned(),
                    output: "NOT EVALUATED: conventional suite unselected".to_owned(),
                });
                if !command.optional_when_not_selected() {
                    expected = false;
                }
            }
            4 => {
                observations.push(GateObservation {
                    command: command.display(),
                    label: command.label().to_owned(),
                    output: "passed without execution".to_owned(),
                });
                expected = false;
            }
            5 => {
                records.push(record(Some(0)));
                records.push(record(Some(0)));
                expected = false;
            }
            _ => unreachable!(),
        }
    }
    if extra_record {
        records.push(GateExecutionRecord {
            command: "unplanned command".to_owned(),
            label: "forged".to_owned(),
            exit_code: Some(0),
            output: String::new(),
        });
        expected = false;
    }
    let constraints = vec![GateExecutionRecord {
        command: "selected artifact exists".to_owned(),
        label: "constraint".to_owned(),
        exit_code: if constraint_passes { Some(0) } else { None },
        output: String::new(),
    }];
    let report = TargetGateReport::from_execution_with_observations(
        &plan,
        records,
        observations,
        constraints,
    );
    assert_eq!(
        report.passed(),
        expected,
        "statuses={statuses:?} optional={optional_mode} extra={extra_record} constraint={constraint_passes}"
    );
    assert_eq!(report.changed_paths(), &[PathBuf::from("alpha.py")]);
    std::fs::remove_file(root.join("alpha.py")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn fixed_report_fail_closed_cases() {
    check(&[1, 1, 1, 1], false, false, true);
    check(&[0, 1, 1, 1], false, false, true);
    check(&[1, 2, 1, 1], false, false, true);
    check(&[1, 3, 1, 1], true, false, true);
    check(&[1, 4, 1, 1], true, false, true);
    check(&[1, 5, 1, 1], false, false, true);
    check(&[1, 1, 1, 1], false, true, true);
    check(&[1, 1, 1, 1], false, false, false);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, .. ProptestConfig::default() })]
    #[test]
    fn generated_report_histories(statuses in prop::collection::vec(0u8..6, 0..8),
        optional_mode in any::<bool>(), extra_record in any::<bool>(),
        constraint_passes in any::<bool>()) {
        check(&statuses, optional_mode, extra_record, constraint_passes);
    }
}
