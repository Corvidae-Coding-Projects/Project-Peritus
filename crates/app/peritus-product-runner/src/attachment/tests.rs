use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
use peritus_model_protocol::{
    CancellationKind, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ResumeKind, StateMode, WireDialect,
};
use peritus_types::ProviderProfileId;

use super::*;

fn profile(images: bool, max_bytes: u64) -> ProviderProfile {
    let supported = if images { vec![Capability::ImageInput] } else { Vec::new() };
    ProviderProfile::new(
        ProviderProfileId::new([0x91; 16]).expect("profile ID"),
        1,
        ProviderName::new("test".to_owned()).expect("provider"),
        ModelName::new("test-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(128_000, 8_192, 32, 1, max_bytes).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let raster = if format == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(RgbImage::new(width, height))
    } else {
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
    };
    let mut bytes = Cursor::new(Vec::new());
    raster.write_to(&mut bytes, format).expect("encode fixture");
    bytes.into_inner()
}

#[test]
fn all_supported_formats_decode_and_retain_exact_original_bytes_and_digest() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif, ImageFormat::WebP] {
        let bytes = encoded(format, 2, 3);
        let original = bytes.clone();
        let image =
            ValidatedImage::decode(bytes, &profile(true, 1024 * 1024)).expect("valid image");
        assert_eq!(image.dimensions(), (2, 3));
        assert_eq!(image.frames(), 1);
        assert_eq!(image.byte_len(), original.len() as u64);
        assert_eq!(image.digest(), Sha256Digest::new(Sha256::digest(&original).into()));
        assert_eq!(image.media().inline_bytes_for_wire(), Some(original.as_slice()));
    }
}

#[test]
fn signatures_without_pixels_and_truncated_pixels_are_not_images() {
    for bytes in [b"\x89PNG\r\n\x1a\n".as_slice(), b"GIF89a", b"\xff\xd8\xff", b"RIFFxxxxWEBP"] {
        assert!(ValidatedImage::decode(bytes.to_vec(), &profile(true, 1024 * 1024)).is_err());
    }
    let mut bytes = encoded(ImageFormat::Png, 2, 3);
    bytes.truncate(bytes.len() / 2);
    assert!(ValidatedImage::decode(bytes, &profile(true, 1024 * 1024)).is_err());
    let error =
        ValidatedImage::decode(b"BMunsupported bitmap".to_vec(), &profile(true, 1024 * 1024))
            .expect_err("unsupported format");
    assert!(error.detail().contains("unsupported image format"));
}

#[test]
fn original_image_admission_is_independent_of_provider_capacity() {
    let bytes = encoded(ImageFormat::Png, 2, 3);
    let len = bytes.len() as u64;
    assert!(ValidatedImage::decode(bytes.clone(), &profile(false, 1024 * 1024)).is_err());
    assert!(ValidatedImage::decode(bytes.clone(), &profile(true, len)).is_ok());
    assert!(ValidatedImage::decode(bytes, &profile(true, 1)).is_ok());
    assert!(ValidatedImage::decode(Vec::new(), &profile(true, 1024 * 1024)).is_err());
}

#[test]
fn original_image_validation_needs_no_provider_profile() {
    let bytes = encoded(ImageFormat::Png, 2, 3);
    let image =
        ValidatedImage::decode_original_with_policy(bytes.clone(), ImageDecodePolicy::default())
            .expect("original validation is provider independent");
    assert_eq!(image.media().inline_bytes_for_wire(), Some(bytes.as_slice()));
}

#[test]
fn decoder_allocation_limit_is_only_applied_when_the_caller_selects_it() {
    let bytes = encoded(ImageFormat::Png, 32, 32);
    assert!(ValidatedImage::decode(bytes.clone(), &profile(true, 1024 * 1024)).is_ok());
    assert!(
        ValidatedImage::decode_with_policy(
            bytes,
            &profile(true, 1024 * 1024),
            ImageDecodePolicy::new(Some(1)),
        )
        .is_err()
    );
}

fn gif(frames: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for _ in 0..frames {
            encoder.encode_frame(image::Frame::new(RgbaImage::new(2, 3))).expect("frame");
        }
    }
    bytes
}

#[test]
fn complete_animation_frames_are_validated_incrementally_without_a_count_ceiling() {
    let image = ValidatedImage::decode(gif(65), &profile(true, 1024 * 1024))
        .expect("frame count exceeds former host quota");
    assert_eq!(image.frames(), 65);
    let mut corrupt = gif(2);
    // Cut inside the second frame's image data, not merely the optional trailer.
    corrupt.truncate(corrupt.len() - 5);
    assert!(ValidatedImage::decode(corrupt, &profile(true, 1024 * 1024)).is_err());
}

#[test]
fn complete_selection_has_no_host_count_or_aggregate_quota() {
    let bytes = encoded(ImageFormat::Png, 2, 3);
    let image = ValidatedImage::decode(bytes, &profile(true, 1024 * 1024)).expect("image");
    assert!(
        validate_image_selection(&vec![image.clone(); 16], &profile(true, 1024 * 1024)).is_ok()
    );
    assert!(
        validate_image_selection(&vec![image.clone(); 17], &profile(true, 1024 * 1024)).is_ok()
    );
    assert!(
        validate_image_selection(std::slice::from_ref(&image), &profile(false, 1024 * 1024))
            .is_err()
    );
    assert!(
        validate_image_selection(
            std::slice::from_ref(&image),
            &profile(true, image.byte_len() - 1)
        )
        .is_err()
    );
    assert!(validate_image_selection(&[], &profile(false, 1024 * 1024)).is_ok());
}
