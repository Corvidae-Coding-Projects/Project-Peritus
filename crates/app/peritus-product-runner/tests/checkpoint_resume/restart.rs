use super::*;

#[test]
fn accepted_candidate_restart_reacquires_only_execution_bound_gates() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x14, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x15, "reviewer", clean_review());
        let first = ProductRunner::run(
            input(
                &repository,
                &state,
                0x16,
                0x17,
                roles(writer.clone(), reviewer, writer.clone()),
                Arc::new(AtomicBool::new(false)),
                SCENARIO_TIMEOUT,
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("initial run");
        assert!(first.settlement().is_accepted());
        let durable = first.resume().expect("qualified continuation").encode_durable().unwrap();
        let resume = peritus_product_runner::ProductRunResume::decode_durable(
            &durable,
            &format!("User:\n{TASK}"),
        )
        .expect("restart continuation");

        let unavailable = scripted(0x18, "provider-must-not-run", Vec::new());
        let phases = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&phases);
        let second = ProductRunner::run(
            input(
                &repository,
                &state,
                0x16,
                0x17,
                roles(unavailable.clone(), unavailable.clone(), unavailable),
                Arc::new(AtomicBool::new(false)),
                SCENARIO_TIMEOUT,
                Some(resume),
            ),
            Arc::new(move |update| observed.lock().expect("phases").push(update.phase)),
        )
        .await
        .expect("gate-only recovery");

        let phases = phases.lock().expect("phases").clone();
        assert!(
            second.settlement().is_accepted(),
            "settlement={:?} detail={:?} remaining={:?} phases={phases:?}",
            second.settlement(),
            second.detail(),
            second.remaining_work(),
        );
        assert!(phases.contains(&ProductRunPhase::Checking));
        assert!(phases.contains(&ProductRunPhase::Finalizing));
        assert!(!phases.iter().any(|phase| matches!(
            phase,
            ProductRunPhase::Designing
                | ProductRunPhase::Writing
                | ProductRunPhase::Reviewing
                | ProductRunPhase::Fixing
        )));
    });
}
