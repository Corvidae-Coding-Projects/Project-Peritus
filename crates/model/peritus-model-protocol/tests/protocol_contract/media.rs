//! Negotiated media ceilings apply to each supported inline payload kind.

use super::*;

#[test]
fn negotiated_inline_media_limit_rejects_oversized_image_audio_and_document_payloads() {
    let profile = profile(1);
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        &profile,
        peritus_model_protocol::RequestedCapabilities::new(
            &all_capabilities(),
            &[],
            ModelLimits::new(80_000, 8_192, 32, 4, 4).expect("four-byte media limit"),
        )
        .expect("capabilities"),
    )
    .expect("negotiation");
    let options = request(&profile, "options").options().clone();
    for kind in [MediaKind::Image, MediaKind::Audio, MediaKind::Document] {
        for bytes in [4, 5] {
            let (mime, wrap): (_, fn(MediaInput) -> ContentBlock) = match kind {
                MediaKind::Image => ("image/png", ContentBlock::Image),
                MediaKind::Audio => ("audio/wav", ContentBlock::Audio),
                MediaKind::Document => ("application/pdf", ContentBlock::Document),
            };
            let media = MediaInput::inline(
                kind,
                MediaType::new(mime.to_owned()).expect("MIME"),
                vec![7; bytes],
                limits,
            )
            .expect("protocol-bounded media");
            let result = ModelRequest::new(
                &profile,
                negotiated,
                RequestId::new("media-bound".to_owned()).expect("id"),
                vec![Message::new(Role::User, vec![wrap(media)], limits).expect("message")],
                Vec::new(),
                ToolChoice::None,
                ParallelToolPolicy::Disabled,
                options.clone(),
                limits,
            );
            if bytes == 4 {
                assert!(result.is_ok(), "exact fit: {result:?}");
            } else {
                assert_eq!(
                    result.expect_err("provider bound must be enforced").kind(),
                    ProtocolErrorKind::InvalidRequest
                );
            }
        }
    }
}
