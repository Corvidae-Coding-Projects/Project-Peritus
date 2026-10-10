//! Incremental pixel validation; metadata alone is not acceptance evidence.

use std::io::Cursor;

use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits};

use super::{ImageDecodePolicy, invalid};
use crate::ProductRunnerError;

pub(super) struct Decoded {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

pub(super) fn validate(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<Decoded, ProductRunnerError> {
    let format = image::guess_format(bytes).map_err(|error| decode_error(&error))?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => return Err(invalid("unsupported image format; expected PNG, JPEG, GIF, or WebP")),
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits(policy));
    let (width, height) = reader.into_dimensions().map_err(|error| decode_error(&error))?;
    let frames = match format {
        ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
                .map_err(|error| decode_error(&error))?;
            decoder.set_limits(limits(policy)).map_err(|error| decode_error(&error))?;
            validate_frames(decoder.into_frames())?
        }
        ImageFormat::Png => {
            let decoder =
                image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), limits(policy))
                    .map_err(|error| decode_error(&error))?;
            // Validate the default image too: APNG may have a separate, hidden default image.
            validate_still(bytes, format, policy)?;
            if decoder.is_apng().map_err(|error| decode_error(&error))? {
                validate_frames(
                    decoder.apng().map_err(|error| decode_error(&error))?.into_frames(),
                )?
            } else {
                1
            }
        }
        ImageFormat::WebP => {
            let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
                .map_err(|error| decode_error(&error))?;
            decoder.set_limits(limits(policy)).map_err(|error| decode_error(&error))?;
            if decoder.has_animation() {
                validate_frames(decoder.into_frames())?
            } else {
                validate_still(bytes, format, policy)?;
                1
            }
        }
        _ => {
            validate_still(bytes, format, policy)?;
            1
        }
    };
    Ok(Decoded { mime, width, height, frames })
}

fn limits(policy: ImageDecodePolicy) -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = policy.max_allocation_bytes();
    limits
}

fn validate_still(
    bytes: &[u8],
    format: ImageFormat,
    policy: ImageDecodePolicy,
) -> Result<(), ProductRunnerError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits(policy));
    reader.decode().map_err(|error| decode_error(&error))?;
    Ok(())
}

fn validate_frames(frames: image::Frames<'_>) -> Result<u32, ProductRunnerError> {
    let mut count = 0_u32;
    for frame in frames {
        count = count.checked_add(1).ok_or_else(|| invalid("animation frame count overflow"))?;
        let _frame = frame.map_err(|error| decode_error(&error))?;
    }
    if count == 0 {
        return Err(invalid("image contains no complete frames"));
    }
    Ok(count)
}

fn decode_error(error: &image::ImageError) -> ProductRunnerError {
    invalid(format!("image pixel validation failed: {error}"))
}
