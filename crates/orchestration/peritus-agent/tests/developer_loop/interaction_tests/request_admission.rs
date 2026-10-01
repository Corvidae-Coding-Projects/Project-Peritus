use super::*;
use peritus_model_protocol::Message;

struct AdmissionRace {
    revision: AtomicU64,
    observed: Mutex<Vec<(u64, peritus_model_protocol::RequestFingerprint)>>,
    fail: bool,
}

impl DeveloperInteraction for AdmissionRace {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(DeveloperInput {
            revision,
            conversation: format!("governing input revision {revision}"),
            images: Vec::new(),
        })
    }

    fn prepare_request(
        &self,
        revision: u64,
        request: &ModelRequest,
    ) -> Result<peritus_agent::DeveloperRequestAdmission, DeveloperLoopError> {
        self.observed.lock().expect("observed").push((revision, request.fingerprint()?));
        if self.fail {
            return Err(DeveloperLoopError::Trace("durable admission failed".to_owned()));
        }
        if self.revision.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            return Ok(peritus_agent::DeveloperRequestAdmission::Stale);
        }
        assert_eq!(revision, 2);
        Ok(peritus_agent::DeveloperRequestAdmission::Accepted)
    }

    fn observe(&self, _: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
}

#[test]
fn exact_prepared_request_is_checked_before_send_and_stale_context_is_rebuilt() {
    block_on(async {
        for fail in [false, true] {
            let provider = ScriptedProvider {
                profile: parallel_profile(),
                responses: Mutex::new(VecDeque::from([text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            let port = AdmissionRace {
                revision: AtomicU64::new(1),
                observed: Mutex::new(Vec::new()),
                fail,
            };
            let outcome = DeveloperLoop::run_interactive(
                &provider,
                request(CancellationToken::new()),
                &mut RecordingTool::default(),
                &mut RecordingTrace::default(),
                None,
                &port,
            )
            .await;
            let sent = provider.requests.lock().expect("sent").clone();
            let prepared = port.observed.lock().expect("prepared").clone();
            if fail {
                assert!(outcome.is_err());
                assert!(sent.is_empty(), "admission failure must not consume a provider request");
                assert_eq!(prepared.len(), 1);
            } else {
                outcome.expect("rebuilt request succeeds");
                assert_eq!(prepared.len(), 2);
                assert_eq!(sent.len(), 1, "stale prepared request must not be sent");
                assert_eq!(prepared[1], (2, sent[0].fingerprint().expect("exact fingerprint")));
                assert_ne!(prepared[0].1, prepared[1].1);
                let visible = sent[0]
                    .messages()
                    .iter()
                    .flat_map(Message::content)
                    .filter_map(|block| {
                        if let ContentBlock::Text(text) = block {
                            Some(text.expose_for_wire())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !visible.contains("governing input revision 1"),
                    "a stale prepared user view must not survive in the rebuilt request"
                );
                assert_eq!(visible.matches("Current governing conversation").count(), 1);
            }
        }
    });
}

struct AdmissionSteeringRace {
    revision: AtomicU64,
    skipped: AtomicU64,
    applied: Mutex<Vec<u64>>,
}

impl DeveloperInteraction for AdmissionSteeringRace {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(DeveloperInput {
            revision,
            conversation: if revision == 1 {
                "Read the file"
            } else {
                "Do not dispatch that tool; answer without it"
            }
            .to_owned(),
            images: Vec::new(),
        })
    }

    fn prepare_request(
        &self,
        revision: u64,
        _: &ModelRequest,
    ) -> Result<peritus_agent::DeveloperRequestAdmission, DeveloperLoopError> {
        self.applied.lock().expect("applied").push(revision);
        Ok(peritus_agent::DeveloperRequestAdmission::Accepted)
    }

    fn admit_tool(
        &self,
        _: peritus_agent::DeveloperModelRole,
        _: &str,
        _: u32,
        input_revision: u64,
        _: peritus_agent::DeveloperToolEffect,
    ) -> Result<peritus_agent::DeveloperControlFlow, DeveloperLoopError> {
        assert_eq!(input_revision, 1);
        self.revision.store(2, Ordering::SeqCst);
        Ok(peritus_agent::DeveloperControlFlow::Yield)
    }

    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        if matches!(activity, DeveloperActivity::ToolSkipped { .. }) {
            self.skipped.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[test]
fn steering_arriving_at_atomic_tool_admission_prevents_dispatch() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let port = AdmissionSteeringRace {
            revision: AtomicU64::new(1),
            skipped: AtomicU64::new(0),
            applied: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        DeveloperLoop::run_interactive(
            &provider,
            request(CancellationToken::new()),
            &mut tools,
            &mut RecordingTrace::default(),
            None,
            &port,
        )
        .await
        .expect("steered admission");

        assert_eq!(tools.calls, 0, "stale tool call must never reach the executor");
        assert_eq!(port.skipped.load(Ordering::SeqCst), 1);
        assert_eq!(*port.applied.lock().expect("applied"), [1, 2]);
        let requests = provider.requests.lock().expect("requests").clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].messages().iter().any(|message| message.content().iter().any(
            |block| {
                matches!(block, ContentBlock::Text(text)
                if text.expose_for_wire().contains("Do not dispatch that tool"))
            }
        )));
    });
}
