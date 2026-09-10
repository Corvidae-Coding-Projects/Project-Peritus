//! Bounded pixel validation; metadata alone is not acceptance evidence.

use std::io::Cursor;

use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits};

use super::{MAX_IMAGE_DECODED_BYTES, MAX_IMAGE_FRAMES, MAX_IMAGE_PIXELS, MAX_IMAGE_SIDE, invalid};
use crate::ProductRunnerError;

pub(super) struct Decoded {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

pub(super) fn validate(bytes: &[u8]) -> Result<Decoded, ProductRunnerError> {
    let format = image::guess_format(bytes).map_err(|error| decode_error(&error))?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => return Err(invalid("unsupported image format; expected PNG, JPEG, GIF, or WebP")),
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let (width, height) = reader.into_dimensions().map_err(|error| decode_error(&error))?;
    check_dimensions(width, height)?;
    let frames = match format {
        ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
                .map_err(|error| decode_error(&error))?;
            decoder.set_limits(limits()).map_err(|error| decode_error(&error))?;
            validate_frames(decoder.into_frames(), 0)?
        }
        ImageFormat::Png => {
            let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), limits())
                .map_err(|error| decode_error(&error))?;
            // Validate the default image too: APNG may have a separate, hidden default image.
            let default_bytes = validate_still(bytes, format)?;
            if decoder.is_apng().map_err(|error| decode_error(&error))? {
                validate_frames(
                    decoder.apng().map_err(|error| decode_error(&error))?.into_frames(),
                    default_bytes,
                )?
            } else {
                1
            }
        }
        ImageFormat::WebP => {
            let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
                .map_err(|error| decode_error(&error))?;
            decoder.set_limits(limits()).map_err(|error| decode_error(&error))?;
            if decoder.has_animation() {
                validate_frames(decoder.into_frames(), 0)?
            } else {
                validate_still(bytes, format)?;
                1
            }
        }
        _ => {
            validate_still(bytes, format)?;
            1
        }
    };
    Ok(Decoded { mime, width, height, frames })
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    // The decoder API's allocation limit is best-effort. Strict dimension/pixel/output checks
    // are separate; this value is not advertised as a hard total process-memory limit.
    limits.max_alloc = Some(MAX_IMAGE_DECODED_BYTES);
    limits
}

fn check_dimensions(width: u32, height: u32) -> Result<(), ProductRunnerError> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_SIDE
        || height > MAX_IMAGE_SIDE
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err(invalid("image dimensions exceed the canvas or pixel limit, or are empty"));
    }
    Ok(())
}

fn validate_still(bytes: &[u8], format: ImageFormat) -> Result<u64, ProductRunnerError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    // ImageReader reserves the output allocation before decoding; from_decoder alone does not.
    let decoded = reader.decode().map_err(|error| decode_error(&error))?;
    check_dimensions(decoded.width(), decoded.height())?;
    let byte_len = u64::try_from(decoded.as_bytes().len())
        .map_err(|_| invalid("decoded image length overflow"))?;
    if byte_len > MAX_IMAGE_DECODED_BYTES {
        return Err(invalid("decoded image exceeds the output byte limit"));
    }
    Ok(byte_len)
}

fn validate_frames(frames: image::Frames<'_>, mut total: u64) -> Result<u32, ProductRunnerError> {
    let mut count = 0_u32;
    for frame in frames {
        count += 1;
        if count > MAX_IMAGE_FRAMES {
            return Err(invalid("image exceeds the animation frame limit"));
        }
        let frame = frame.map_err(|error| decode_error(&error))?;
        check_dimensions(frame.buffer().width(), frame.buffer().height())?;
        let bytes = u64::try_from(frame.buffer().as_raw().len())
            .map_err(|_| invalid("decoded frame length overflow"))?;
        total =
            total.checked_add(bytes).ok_or_else(|| invalid("decoded animation length overflow"))?;
        if total > MAX_IMAGE_DECODED_BYTES {
            return Err(invalid("animation exceeds the decoded output byte limit"));
        }
    }
    if count == 0 {
        return Err(invalid("image contains no complete frames"));
    }
    Ok(count)
}

fn decode_error(error: &image::ImageError) -> ProductRunnerError {
    invalid(format!("image pixel validation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_frame() -> image::Frames<'static> {
        image::Frames::new(Box::new(std::iter::once(Ok(image::Frame::new(image::RgbaImage::new(
            2, 3,
        ))))))
    }

    #[test]
    fn prior_default_image_output_is_conserved_in_animation_budget() {
        assert_eq!(
            validate_frames(single_frame(), MAX_IMAGE_DECODED_BYTES - 24).expect("exact bound"),
            1
        );
        let error = validate_frames(single_frame(), MAX_IMAGE_DECODED_BYTES - 23)
            .expect_err("one output byte over");
        assert!(error.detail().contains("output byte limit"));
        assert!(validate_frames(image::Frames::new(Box::new(std::iter::empty())), 0).is_err());
    }
}
