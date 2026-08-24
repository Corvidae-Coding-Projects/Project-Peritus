use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use peritus_model_protocol::{FailureCategory, ModelEvent};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, Header, HeaderName, HttpHeaders, HttpLimits,
    HttpRequest, HttpResponse, HttpTransport, MemoryByteStream, ModelProvider, ProviderCoreError,
    RetryPolicy, StatusCode,
};

use super::support::{
    StaticCredential, block_on, credential_reference, fixture, minimal_request, profile_minimal,
};
use crate::{OpenAiConfig, OpenAiProvider};

enum Reply {
    Connect,
    Ambiguous,
    Status(u16, Vec<u8>),
    Success,
}

struct SequenceTransport {
    replies: Mutex<VecDeque<Reply>>,
    sends: AtomicU64,
}

impl SequenceTransport {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self { replies: Mutex::new(replies.into_iter().collect()), sends: AtomicU64::new(0) }
    }
}

impl HttpTransport for SequenceTransport {
    fn send<'a>(
        &'a self,
        _request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        let reply = self.replies.lock().expect("replies").pop_front().expect("scripted reply");
        Box::pin(async move {
            match reply {
                Reply::Connect => Err(ProviderCoreError::connect("test", "connect failed")),
                Reply::Ambiguous => {
                    Err(ProviderCoreError::transport("test", "submission interrupted"))
                }
                Reply::Status(status, body) => response(status, &body, false),
                Reply::Success => response(200, &fixture("success.sse"), true),
            }
        })
    }
}

fn response(
    status: u16,
    bytes: &[u8],
    event_stream: bool,
) -> Result<HttpResponse, ProviderCoreError> {
    let limits = HttpLimits::PRODUCTION;
    let headers = if event_stream {
        HttpHeaders::new(
            vec![Header::new(
                HeaderName::new("content-type".to_owned())?,
                b"text/event-stream".to_vec(),
            )?],
            limits,
        )?
    } else {
        HttpHeaders::new(
            vec![Header::new(
                HeaderName::new("x-request-id".to_owned())?,
                b"retry-request-id".to_vec(),
            )?],
            limits,
        )?
    };
    let body = MemoryByteStream::new(vec![bytes.to_vec()], limits)?;
    HttpResponse::new(StatusCode::new(status)?, headers, Box::new(body), limits)
}

async fn exercise(replies: impl IntoIterator<Item = Reply>) -> (Vec<ModelEvent>, u64, u64) {
    let profile = profile_minimal();
    let request = minimal_request(&profile);
    let policy = RetryPolicy::new(
        2,
        [
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_secs(1),
        ],
        1024 * 1024,
    )
    .expect("retry policy");
    let config = OpenAiConfig::for_test(
        Endpoint::new("http://127.0.0.1:9".to_owned()).expect("endpoint"),
        credential_reference(),
    )
    .expect("config")
    .with_retry_policy(policy);
    let credentials = Arc::new(StaticCredential::new());
    let transport = Arc::new(SequenceTransport::new(replies));
    let provider =
        OpenAiProvider::with_transport(config, profile, credentials.clone(), transport.clone())
            .expect("provider");
    let mut stream = provider.start(request, CancellationToken::new()).await.expect("start");
    let mut events = Vec::new();
    while let Some(event) = stream.pull().await.expect("pull") {
        events.push(event.event().clone());
    }
    (events, transport.sends.load(Ordering::SeqCst), credentials.resolutions())
}

#[test]
fn connect_rate_limit_and_server_rejections_retry_but_ambiguous_and_quota_do_not() {
    block_on(async {
        for first in [
            Reply::Connect,
            Reply::Status(429, fixture("rate-error.json")),
            Reply::Status(503, fixture("transient-error.json")),
        ] {
            let (events, sends, resolutions) = exercise([first, Reply::Success]).await;
            assert_eq!((sends, resolutions), (2, 2));
            assert!(events.iter().any(|event| matches!(event, ModelEvent::ResponseCompleted)));
        }

        let (ambiguous, sends, _) = exercise([Reply::Ambiguous]).await;
        assert_eq!(sends, 1);
        assert!(matches!(
            ambiguous.first(),
            Some(ModelEvent::ResponseFailed(failure))
                if failure.category() == FailureCategory::AmbiguousAcceptance
        ));

        let (quota, sends, _) = exercise([Reply::Status(429, fixture("quota-error.json"))]).await;
        assert_eq!(sends, 1);
        assert!(matches!(
            quota.first(),
            Some(ModelEvent::ResponseFailed(failure))
                if failure.category() == FailureCategory::QuotaExhausted
                    && failure.response_id().map(
                        peritus_model_protocol::ResponseId::expose_for_wire
                    ) == Some("retry-request-id")
        ));
    });
}
