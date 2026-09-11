use super::*;
use peritus_model_protocol::{MediaInput, MediaKind, MediaType};

struct ImageInput {
    revision: AtomicU64,
    withdraw_at_admission: bool,
    prepared: Mutex<Vec<usize>>,
}
fn image() -> MediaInput {
    MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("MIME"),
        b"exact image bytes selected by host".to_vec(),
        ProtocolLimits::PRODUCTION,
    )
    .expect("media")
}
fn images(request: &ModelRequest) -> Vec<MediaInput> {
    request
        .messages()
        .iter()
        .flat_map(Message::content)
        .filter_map(
            |block| {
                if let ContentBlock::Image(media) = block { Some(media.clone()) } else { None }
            },
        )
        .collect()
}
impl DeveloperInteraction for ImageInput {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(DeveloperInput {
            revision,
            conversation: format!("Confirmed input {revision}"),
            images: if revision == 1 { vec![image()] } else { Vec::new() },
        })
    }
    fn prepare_request(
        &self,
        revision: u64,
        request: &ModelRequest,
    ) -> Result<peritus_agent::DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepared.lock().expect("prepared").push(images(request).len());
        if self.withdraw_at_admission
            && self.revision.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst).is_ok()
        {
            return Ok(peritus_agent::DeveloperRequestAdmission::Stale);
        }
        assert_eq!(revision, self.revision.load(Ordering::SeqCst));
        Ok(peritus_agent::DeveloperRequestAdmission::Accepted)
    }
    fn observe(&self, _: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
}

#[test]
fn exact_current_images_follow_governing_input_and_withdrawal_discards_the_stale_image() {
    block_on(async {
        for withdraw_at_admission in [false, true] {
            let provider = ScriptedProvider {
                profile: fixtures::image_profile(),
                responses: Mutex::new(VecDeque::from([text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            let port = ImageInput {
                revision: AtomicU64::new(1),
                withdraw_at_admission,
                prepared: Mutex::new(Vec::new()),
            };
            DeveloperLoop::run_interactive(
                &provider,
                request(CancellationToken::new()),
                &mut RecordingTool::default(),
                &mut RecordingTrace::default(),
                None,
                &port,
            )
            .await
            .expect("image request");
            let requests = provider.requests.lock().expect("requests").clone();
            assert_eq!(requests.len(), 1, "stale request is never sent");
            if withdraw_at_admission {
                assert_eq!(*port.prepared.lock().expect("prepared"), [1, 0]);
                assert!(
                    images(&requests[0]).is_empty(),
                    "withdrawn bytes must not survive rebuild"
                );
            } else {
                assert_eq!(*port.prepared.lock().expect("prepared"), [1]);
                assert_eq!(images(&requests[0]), vec![image()]);
            }
        }
    });
}

#[test]
fn unsupported_live_image_input_rejects_before_incorporation_and_provider_send() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let port = ImageInput {
            revision: AtomicU64::new(1),
            withdraw_at_admission: false,
            prepared: Mutex::new(Vec::new()),
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
        assert!(outcome.is_err());
        assert!(port.prepared.lock().expect("prepared").is_empty());
        assert!(provider.requests.lock().expect("requests").is_empty());
    });
}
