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
            ValidatedImage::decode(bytes, &profile(true, MAX_IMAGE_BYTES)).expect("valid image");
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
        assert!(ValidatedImage::decode(bytes.to_vec(), &profile(true, MAX_IMAGE_BYTES)).is_err());
    }
    let mut bytes = encoded(ImageFormat::Png, 2, 3);
    bytes.truncate(bytes.len() / 2);
    assert!(ValidatedImage::decode(bytes, &profile(true, MAX_IMAGE_BYTES)).is_err());
    let error =
        ValidatedImage::decode(b"BMunsupported bitmap".to_vec(), &profile(true, MAX_IMAGE_BYTES))
            .expect_err("unsupported format");
    assert!(error.detail().contains("unsupported image format"));
}

#[test]
fn provider_capability_and_exact_encoded_byte_bound_are_enforced() {
    let bytes = encoded(ImageFormat::Png, 2, 3);
    let len = bytes.len() as u64;
    assert!(ValidatedImage::decode(bytes.clone(), &profile(false, MAX_IMAGE_BYTES)).is_err());
    assert!(ValidatedImage::decode(bytes.clone(), &profile(true, len)).is_ok());
    assert!(ValidatedImage::decode(bytes, &profile(true, len - 1)).is_err());
    assert!(ValidatedImage::decode(Vec::new(), &profile(true, MAX_IMAGE_BYTES)).is_err());
    assert!(
        ValidatedImage::decode(
            vec![0; usize::try_from(MAX_IMAGE_BYTES).expect("host byte limit") + 1],
            &profile(true, MAX_IMAGE_BYTES * 2)
        )
        .is_err()
    );
}

#[test]
fn image_dimensions_reject_a_small_encoded_bomb_before_pixel_decode() {
    let bytes = encoded(ImageFormat::Png, MAX_IMAGE_SIDE + 1, 1);
    let error =
        ValidatedImage::decode(bytes, &profile(true, MAX_IMAGE_BYTES)).expect_err("width bound");
    assert!(error.detail().contains("limit"));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(image::GrayImage::new(4097, 4096))
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("pixel bomb fixture");
    let error = ValidatedImage::decode(bytes.into_inner(), &profile(true, MAX_IMAGE_BYTES))
        .expect_err("pixel bound");
    assert!(error.detail().contains("pixel limit"));
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
fn complete_animation_frames_are_validated_with_an_exact_count_bound() {
    let image = ValidatedImage::decode(gif(MAX_IMAGE_FRAMES), &profile(true, MAX_IMAGE_BYTES))
        .expect("frame boundary");
    assert_eq!(image.frames(), MAX_IMAGE_FRAMES);
    let error = ValidatedImage::decode(gif(MAX_IMAGE_FRAMES + 1), &profile(true, MAX_IMAGE_BYTES))
        .expect_err("frame limit");
    assert!(error.detail().contains("frame limit"));
    let mut corrupt = gif(2);
    // Cut inside the second frame's image data, not merely the optional trailer.
    corrupt.truncate(corrupt.len() - 5);
    assert!(ValidatedImage::decode(corrupt, &profile(true, MAX_IMAGE_BYTES)).is_err());
}

#[test]
fn complete_selection_rejects_count_aggregate_and_changed_provider_without_omission() {
    let bytes = encoded(ImageFormat::Png, 2, 3);
    let image = ValidatedImage::decode(bytes, &profile(true, MAX_IMAGE_BYTES)).expect("image");
    assert!(
        validate_image_selection(
            &vec![image.clone(); MAX_IMAGE_COUNT],
            &profile(true, MAX_IMAGE_BYTES)
        )
        .is_ok()
    );
    assert!(
        validate_image_selection(
            &vec![image.clone(); MAX_IMAGE_COUNT + 1],
            &profile(true, MAX_IMAGE_BYTES)
        )
        .is_err()
    );
    assert!(
        validate_image_selection(std::slice::from_ref(&image), &profile(false, MAX_IMAGE_BYTES))
            .is_err()
    );
    assert!(
        validate_image_selection(
            std::slice::from_ref(&image),
            &profile(true, image.byte_len() - 1)
        )
        .is_err()
    );
    assert!(validate_image_selection(&[], &profile(false, MAX_IMAGE_BYTES)).is_ok());
    let mut bytes = encoded(ImageFormat::Png, 2, 3);
    bytes.resize(usize::try_from(MAX_IMAGE_BYTES).expect("host byte limit"), 0);
    let large = ValidatedImage::decode(bytes, &profile(true, MAX_IMAGE_BYTES))
        .expect("bounded padded image");
    assert!(
        validate_image_selection(&vec![large.clone(); 3], &profile(true, MAX_IMAGE_BYTES)).is_ok()
    );
    assert!(validate_image_selection(&vec![large; 4], &profile(true, MAX_IMAGE_BYTES)).is_err());
}
