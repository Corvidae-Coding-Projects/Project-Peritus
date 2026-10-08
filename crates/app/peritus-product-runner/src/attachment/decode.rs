//! Streamed container validation with optional bounded complete-pixel evidence.

use std::io::{BufReader, Read, Seek, SeekFrom};

use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits};

use super::{ImageValidation, MAX_IMAGE_DECODE_ALLOCATION_BYTES, invalid};
use crate::ProductRunnerError;

mod structure;

pub(super) struct Decoded {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub validation: ImageValidation,
}

pub(super) fn validate<R: Read + Seek>(reader: &mut R) -> Result<Decoded, ProductRunnerError> {
    let format = detected_format(reader)?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => return Err(invalid("unsupported image format; expected PNG, JPEG, GIF, or WebP")),
    };
    let inspected = structure::inspect(reader, format)?;
    let validation = if pixel_window_fits(inspected.width, inspected.height) {
        match validate_pixels(reader, format) {
            Ok(decoded)
                if decoded
                    == (inspected.width, inspected.height, inspected.frames) =>
            {
                ImageValidation::CompletePixels
            }
            Ok(_) => return Err(invalid("decoded pixels differ from the inspected image structure")),
            Err(PixelFailure::Unavailable) => ImageValidation::ContainerStructure,
            Err(PixelFailure::Invalid(error)) => return Err(error),
        }
    } else {
        ImageValidation::ContainerStructure
    };
    Ok(Decoded {
        mime,
        width: inspected.width,
        height: inspected.height,
        frames: inspected.frames,
        validation,
    })
}

fn validate_pixels<R: Read + Seek>(
    reader: &mut R,
    format: ImageFormat,
) -> Result<(u32, u32, u32), PixelFailure> {
    let (width, height, frames) = match format {
        ImageFormat::Gif => {
            rewind_pixels(reader)?;
            let mut decoder = image::codecs::gif::GifDecoder::new(BufReader::new(&mut *reader))
                .map_err(pixel_error)?;
            decoder.set_limits(limits()).map_err(pixel_error)?;
            let dimensions = decoder.dimensions();
            (dimensions.0, dimensions.1, validate_frames(decoder.into_frames())?)
        }
        ImageFormat::Png => {
            // Validate the default image too: APNG may carry it separately from animation frames.
            let dimensions = validate_still(reader, format)?;
            rewind_pixels(reader)?;
            let decoder = image::codecs::png::PngDecoder::with_limits(
                BufReader::new(&mut *reader),
                limits(),
            )
            .map_err(pixel_error)?;
            let frames = if decoder.is_apng().map_err(pixel_error)? {
                validate_frames(decoder.apng().map_err(pixel_error)?.into_frames())?
            } else {
                1
            };
            (dimensions.0, dimensions.1, frames)
        }
        ImageFormat::WebP => {
            rewind_pixels(reader)?;
            let mut decoder =
                image::codecs::webp::WebPDecoder::new(BufReader::new(&mut *reader))
                    .map_err(pixel_error)?;
            decoder.set_limits(limits()).map_err(pixel_error)?;
            let dimensions = decoder.dimensions();
            if decoder.has_animation() {
                (dimensions.0, dimensions.1, validate_frames(decoder.into_frames())?)
            } else {
                drop(decoder);
                let dimensions = validate_still(reader, format)?;
                (dimensions.0, dimensions.1, 1)
            }
        }
        _ => {
            let dimensions = validate_still(reader, format)?;
            (dimensions.0, dimensions.1, 1)
        }
    };
    Ok((width, height, frames))
}

fn detected_format<R: Read + Seek>(reader: &mut R) -> Result<ImageFormat, ProductRunnerError> {
    rewind(reader)?;
    let mut header = [0_u8; 32];
    let mut length = 0_usize;
    while length < header.len() {
        let count = reader.read(&mut header[length..]).map_err(io_error)?;
        if count == 0 {
            break;
        }
        length += count;
    }
    rewind(reader)?;
    image::guess_format(&header[..length]).map_err(|error| decode_error(&error))
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    // This governs optional evidence only. A limit result preserves complete container evidence;
    // it never rejects an attachment or becomes a logical image-shape allowance.
    limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOCATION_BYTES);
    limits
}

fn pixel_window_fits(width: u32, height: u32) -> bool {
    u64::from(width)
        .checked_mul(u64::from(height))
        // PNG can carry 16-bit RGBA output. This conservative bound also protects image 0.25's
        // WebP paths that do not consistently consult Limits before creating output buffers.
        .and_then(|pixels| pixels.checked_mul(8))
        .is_some_and(|bytes| bytes <= MAX_IMAGE_DECODE_ALLOCATION_BYTES)
}

fn validate_still<R: Read + Seek>(
    reader: &mut R,
    format: ImageFormat,
) -> Result<(u32, u32), PixelFailure> {
    rewind_pixels(reader)?;
    let mut image = ImageReader::with_format(BufReader::new(&mut *reader), format);
    image.limits(limits());
    let decoded = image.decode().map_err(pixel_error)?;
    Ok((decoded.width(), decoded.height()))
}

fn validate_frames(frames: image::Frames<'_>) -> Result<u32, PixelFailure> {
    let mut count = 0_u32;
    for frame in frames {
        let frame = frame.map_err(pixel_error)?;
        if frame.buffer().width() == 0 || frame.buffer().height() == 0 {
            return Err(PixelFailure::Invalid(invalid("decoded image frame is empty")));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| PixelFailure::Invalid(invalid("animation frame count is not representable")))?;
        drop(frame);
    }
    if count == 0 {
        return Err(PixelFailure::Invalid(invalid("image contains no complete pixel frames")));
    }
    Ok(count)
}

fn rewind<R: Seek>(reader: &mut R) -> Result<(), ProductRunnerError> {
    reader.seek(SeekFrom::Start(0)).map(|_| ()).map_err(io_error)
}

fn rewind_pixels<R: Seek>(reader: &mut R) -> Result<(), PixelFailure> {
    rewind(reader).map_err(PixelFailure::Invalid)
}

fn io_error(error: std::io::Error) -> ProductRunnerError {
    invalid(format!("image source read failed: {error}"))
}

fn decode_error(error: &image::ImageError) -> ProductRunnerError {
    invalid(format!("image pixel validation failed: {error}"))
}

enum PixelFailure {
    Unavailable,
    Invalid(ProductRunnerError),
}

fn pixel_error(error: image::ImageError) -> PixelFailure {
    if matches!(
        &error,
        image::ImageError::Limits(_) | image::ImageError::Unsupported(_)
    ) {
        PixelFailure::Unavailable
    } else {
        PixelFailure::Invalid(decode_error(&error))
    }
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
