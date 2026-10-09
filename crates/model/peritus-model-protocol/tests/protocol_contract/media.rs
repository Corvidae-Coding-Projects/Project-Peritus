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

#[test]
fn selected_provider_capacity_allows_media_above_the_old_global_inline_ceiling() {
    let provider = ProviderProfile::new(
        ProviderProfileId::new([8; 16]).expect("profile id"),
        1,
        ProviderName::new("large-media-provider".to_owned()).expect("provider"),
        ModelName::new("large-media-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&all_capabilities(), &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(100_000, 16_384, 64, 8, 40 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::Confirmed,
    )
    .expect("profile");
    let requested = peritus_model_protocol::RequestedCapabilities::new(
        &all_capabilities(),
        &[],
        ModelLimits::new(80_000, 8_192, 32, 4, 40 * 1024 * 1024).expect("requested limits"),
    )
    .expect("requested capabilities");
    let negotiated = negotiate(&provider, requested).expect("negotiation");
    let limits = ProtocolLimits::PRODUCTION;
    let media = MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("MIME"),
        vec![7; 33 * 1024 * 1024],
        ProtocolLimits::ARCHIVE,
    )
    .expect("archived original media");
    let message =
        Message::new(Role::User, vec![ContentBlock::Image(media)], limits).expect("message");
    let result = ModelRequest::new(
        &provider,
        negotiated,
        RequestId::new("large-image".to_owned()).expect("request id"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        request(&provider, "large-image-options").options().clone(),
        limits,
    );
    assert!(result.is_ok(), "selected provider capacity governs this request: {result:?}");
}
