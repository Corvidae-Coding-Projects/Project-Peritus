//! Complete encoded-container inspection without allocating a decoded pixel canvas.

use std::io::{BufReader, Read, Seek, SeekFrom};

use image::ImageFormat;

use super::super::invalid;
use crate::ProductRunnerError;

pub(super) struct Structure {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

pub(super) fn inspect<R: Read + Seek>(
    reader: &mut R,
    format: ImageFormat,
) -> Result<Structure, ProductRunnerError> {
    reader.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let length = reader.seek(SeekFrom::End(0)).map_err(io_error)?;
    reader.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut buffered = BufReader::new(reader);
    let inspected = match format {
        ImageFormat::Png => png(&mut buffered),
        ImageFormat::Jpeg => jpeg(&mut buffered),
        ImageFormat::Gif => gif(&mut buffered),
        ImageFormat::WebP => webp(&mut buffered, length),
        _ => Err(invalid("unsupported image format; expected PNG, JPEG, GIF, or WebP")),
    }?;
    let mut trailing = [0_u8; 1];
    if buffered.read(&mut trailing).map_err(io_error)? != 0 {
        return Err(corrupt("trailing bytes follow the image terminator"));
    }
    Ok(inspected)
}

fn png(reader: &mut impl Read) -> Result<Structure, ProductRunnerError> {
    if bytes::<8>(reader)? != *b"\x89PNG\r\n\x1a\n" {
        return Err(corrupt("invalid PNG signature"));
    }
    let mut index = 0_u64;
    let mut dimensions = None;
    let mut color_type = None;
    let mut palette = false;
    let mut image_data = false;
    let mut image_data_bytes = 0_u64;
    let mut image_data_ended = false;
    let mut animation_frames = None;
    let mut frame_controls = 0_u32;
    let mut animation_sequence = None;
    let mut current_frame_has_data = false;
    let mut current_frame_expects_fdat = false;
    loop {
        let length = u64::from(u32::from_be_bytes(bytes::<4>(reader)?));
        let kind = bytes::<4>(reader)?;
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(corrupt("invalid PNG chunk type"));
        }
        if length > 0x7fff_ffff {
            return Err(corrupt("PNG chunk exceeds the format length limit"));
        }
        if kind[2].is_ascii_lowercase() {
            return Err(corrupt("PNG chunk uses the reserved type bit"));
        }
        if index == 0 && kind != *b"IHDR" {
            return Err(corrupt("PNG header is not the first chunk"));
        }
        index = index.checked_add(1).ok_or_else(|| corrupt("PNG chunk count overflow"))?;
        let mut capture = [0_u8; 26];
        let captured = read_png_chunk(reader, kind, length, &mut capture)?;
        match &kind {
            b"IHDR" => {
                if dimensions.is_some() || length != 13 || captured != 13 {
                    return Err(corrupt("invalid PNG header chunk"));
                }
                let width = u32::from_be_bytes(capture[0..4].try_into().map_err(|_| corrupt("invalid PNG width"))?);
                let height = u32::from_be_bytes(capture[4..8].try_into().map_err(|_| corrupt("invalid PNG height"))?);
                check_dimensions(width, height)?;
                if !valid_png_depth(capture[8], capture[9])
                    || capture[10] != 0
                    || capture[11] != 0
                    || capture[12] > 1
                {
                    return Err(corrupt("unsupported PNG header fields"));
                }
                dimensions = Some((width, height));
                color_type = Some(capture[9]);
            }
            b"PLTE" => {
                if dimensions.is_none()
                    || image_data
                    || palette
                    || length == 0
                    || length > 768
                    || length % 3 != 0
                    || matches!(color_type, Some(0 | 4))
                {
                    return Err(corrupt("invalid PNG palette chunk"));
                }
                palette = true;
            }
            b"IDAT" => {
                if dimensions.is_none() || image_data_ended {
                    return Err(corrupt("invalid PNG image-data ordering"));
                }
                if color_type == Some(3) && !palette {
                    return Err(corrupt("indexed PNG has no palette"));
                }
                image_data = true;
                if frame_controls != 0 {
                    if current_frame_expects_fdat {
                        return Err(corrupt("PNG animation frame mixes IDAT and fdAT data"));
                    }
                    current_frame_has_data = true;
                }
                image_data_bytes = image_data_bytes
                    .checked_add(length)
                    .ok_or_else(|| corrupt("PNG image-data length overflow"))?;
            }
            b"acTL" => {
                if dimensions.is_none() || image_data || animation_frames.is_some() || length != 8 {
                    return Err(corrupt("invalid PNG animation control"));
                }
                let frames = u32::from_be_bytes(capture[0..4].try_into().map_err(|_| corrupt("invalid PNG frame count"))?);
                if frames == 0 {
                    return Err(corrupt("PNG animation declares no frames"));
                }
                animation_frames = Some(frames);
            }
            b"fcTL" => {
                let canvas = dimensions.ok_or_else(|| corrupt("PNG frame precedes header"))?;
                if animation_frames.is_none() || length != 26 {
                    return Err(corrupt("invalid PNG frame control"));
                }
                if frame_controls != 0 && !current_frame_has_data {
                    return Err(corrupt("PNG animation frame has no image data"));
                }
                advance_png_sequence(&mut animation_sequence, &capture)?;
                let width = u32::from_be_bytes(capture[4..8].try_into().map_err(|_| corrupt("invalid PNG frame width"))?);
                let height = u32::from_be_bytes(capture[8..12].try_into().map_err(|_| corrupt("invalid PNG frame height"))?);
                let x = u32::from_be_bytes(capture[12..16].try_into().map_err(|_| corrupt("invalid PNG frame x offset"))?);
                let y = u32::from_be_bytes(capture[16..20].try_into().map_err(|_| corrupt("invalid PNG frame y offset"))?);
                check_frame(canvas, (x, y), (width, height))?;
                if capture[24] > 2 || capture[25] > 1 {
                    return Err(corrupt("invalid PNG animation frame operations"));
                }
                frame_controls = frame_controls
                    .checked_add(1)
                    .ok_or_else(|| corrupt("PNG frame count overflow"))?;
                current_frame_has_data = false;
                current_frame_expects_fdat = image_data;
            }
            b"fdAT" => {
                if animation_frames.is_none()
                    || frame_controls == 0
                    || !image_data
                    || !current_frame_expects_fdat
                    || length <= 4
                {
                    return Err(corrupt("invalid PNG animation data"));
                }
                advance_png_sequence(&mut animation_sequence, &capture)?;
                current_frame_has_data = true;
            }
            b"IEND" => {
                let (width, height) = dimensions.ok_or_else(|| corrupt("PNG has no header"))?;
                if length != 0 || !image_data || image_data_bytes == 0 {
                    return Err(corrupt("PNG terminates without complete image data"));
                }
                let frames = match animation_frames {
                    Some(frames) if frames == frame_controls && current_frame_has_data => frames,
                    Some(_) if !current_frame_has_data => {
                        return Err(corrupt("PNG animation frame has no image data"));
                    }
                    Some(_) => return Err(corrupt("PNG animation frame count differs from its control")),
                    None => 1,
                };
                return Ok(Structure { width, height, frames });
            }
            _ => {
                if kind[0].is_ascii_uppercase() {
                    return Err(corrupt("unsupported critical PNG chunk"));
                }
            }
        }
        if image_data && kind != *b"IDAT" {
            image_data_ended = true;
        }
    }
}

fn read_png_chunk(
    reader: &mut impl Read,
    kind: [u8; 4],
    length: u64,
    capture: &mut [u8],
) -> Result<usize, ProductRunnerError> {
    let mut crc = crc32_start();
    crc = crc32_update(crc, &kind);
    let mut remaining = length;
    let mut copied = 0_usize;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| corrupt("PNG chunk length is not representable"))?;
        reader.read_exact(&mut buffer[..count]).map_err(io_error)?;
        crc = crc32_update(crc, &buffer[..count]);
        let retained = (capture.len() - copied).min(count);
        capture[copied..copied + retained].copy_from_slice(&buffer[..retained]);
        copied += retained;
        remaining -= u64::try_from(count).map_err(|_| corrupt("PNG chunk length overflow"))?;
    }
    let observed = u32::from_be_bytes(bytes::<4>(reader)?);
    if observed != crc32_finish(crc) {
        return Err(corrupt("PNG chunk checksum mismatch"));
    }
    Ok(copied)
}

fn valid_png_depth(depth: u8, color: u8) -> bool {
    match color {
        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
        2 | 4 | 6 => matches!(depth, 8 | 16),
        3 => matches!(depth, 1 | 2 | 4 | 8),
        _ => false,
    }
}

fn advance_png_sequence(
    prior: &mut Option<u32>,
    bytes: &[u8],
) -> Result<(), ProductRunnerError> {
    let current = u32::from_be_bytes(
        bytes[..4].try_into().map_err(|_| corrupt("invalid PNG animation sequence"))?,
    );
    match *prior {
        None if current == 0 => *prior = Some(current),
        Some(value) if current == value.checked_add(1).ok_or_else(|| corrupt("PNG animation sequence overflow"))? => {
            *prior = Some(current);
        }
        _ => return Err(corrupt("non-contiguous PNG animation sequence")),
    }
    Ok(())
}

fn jpeg(reader: &mut impl Read) -> Result<Structure, ProductRunnerError> {
    if bytes::<2>(reader)? != [0xff, 0xd8] {
        return Err(corrupt("invalid JPEG start marker"));
    }
    let mut dimensions = None;
    let mut scans = 0_u32;
    let mut pending = None;
    loop {
        let marker = match pending.take() {
            Some(marker) => marker,
            None => jpeg_marker(reader)?,
        };
        match marker {
            0xd9 => {
                let (width, height) = dimensions.ok_or_else(|| corrupt("JPEG has no frame header"))?;
                if scans == 0 {
                    return Err(corrupt("JPEG has no image scan"));
                }
                return Ok(Structure { width, height, frames: 1 });
            }
            0xda => {
                if dimensions.is_none() {
                    return Err(corrupt("JPEG image scan precedes its frame header"));
                }
                let length = jpeg_segment_length(reader)?;
                validate_jpeg_scan_header(reader, length)?;
                scans = scans.checked_add(1).ok_or_else(|| corrupt("JPEG scan count overflow"))?;
                pending = Some(jpeg_entropy_marker(reader)?);
            }
            marker if jpeg_sof(marker) => {
                if dimensions.is_some() {
                    return Err(corrupt("JPEG has multiple frame headers"));
                }
                let length = jpeg_segment_length(reader)?;
                if length < 6 {
                    return Err(corrupt("truncated JPEG frame header"));
                }
                let header = bytes::<6>(reader)?;
                let height = u32::from(u16::from_be_bytes([header[1], header[2]]));
                let width = u32::from(u16::from_be_bytes([header[3], header[4]]));
                check_dimensions(width, height)?;
                let expected = 6_u64
                    .checked_add(u64::from(header[5]) * 3)
                    .ok_or_else(|| corrupt("JPEG frame-header length overflow"))?;
                if header[5] == 0 || length != expected {
                    return Err(corrupt("invalid JPEG frame-header component table"));
                }
                skip(reader, length - 6)?;
                dimensions = Some((width, height));
            }
            0x01 => {}
            0xd8 | 0xd0..=0xd7 => {
                return Err(corrupt("unexpected standalone JPEG marker"));
            }
            _ => {
                let length = jpeg_segment_length(reader)?;
                skip(reader, length)?;
            }
        }
    }
}

fn jpeg_marker(reader: &mut impl Read) -> Result<u8, ProductRunnerError> {
    if byte(reader)? != 0xff {
        return Err(corrupt("JPEG segment is not marker-aligned"));
    }
    loop {
        let marker = byte(reader)?;
        match marker {
            0xff => {}
            0x00 => return Err(corrupt("stuffed JPEG byte outside image scan")),
            _ => return Ok(marker),
        }
    }
}

fn validate_jpeg_scan_header(
    reader: &mut impl Read,
    length: u64,
) -> Result<(), ProductRunnerError> {
    if length < 4 {
        return Err(corrupt("truncated JPEG scan header"));
    }
    let components = byte(reader)?;
    let expected = 1_u64
        .checked_add(u64::from(components) * 2)
        .and_then(|value| value.checked_add(3))
        .ok_or_else(|| corrupt("JPEG scan-header length overflow"))?;
    if components == 0 || length != expected {
        return Err(corrupt("invalid JPEG scan-header component table"));
    }
    skip(reader, length - 1)
}

fn jpeg_entropy_marker(reader: &mut impl Read) -> Result<u8, ProductRunnerError> {
    loop {
        if byte(reader)? != 0xff {
            continue;
        }
        loop {
            match byte(reader)? {
                0x00 => break,
                0xff => {}
                0xd0..=0xd7 => break,
                marker => return Ok(marker),
            }
        }
    }
}

fn jpeg_segment_length(reader: &mut impl Read) -> Result<u64, ProductRunnerError> {
    let length = u16::from_be_bytes(bytes::<2>(reader)?);
    if length < 2 {
        return Err(corrupt("invalid JPEG segment length"));
    }
    Ok(u64::from(length - 2))
}

fn jpeg_sof(marker: u8) -> bool {
    matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
}

fn gif(reader: &mut impl Read) -> Result<Structure, ProductRunnerError> {
    let signature = bytes::<6>(reader)?;
    if signature != *b"GIF87a" && signature != *b"GIF89a" {
        return Err(corrupt("invalid GIF signature"));
    }
    let screen = bytes::<7>(reader)?;
    let width = u32::from(u16::from_le_bytes([screen[0], screen[1]]));
    let height = u32::from(u16::from_le_bytes([screen[2], screen[3]]));
    check_dimensions(width, height)?;
    let global_palette = screen[4] & 0x80 != 0;
    if global_palette {
        skip(reader, gif_color_table_bytes(screen[4]))?;
    }
    let mut frames = 0_u32;
    loop {
        match byte(reader)? {
            0x2c => {
                let descriptor = bytes::<9>(reader)?;
                let x = u32::from(u16::from_le_bytes([descriptor[0], descriptor[1]]));
                let y = u32::from(u16::from_le_bytes([descriptor[2], descriptor[3]]));
                let frame_width = u32::from(u16::from_le_bytes([descriptor[4], descriptor[5]]));
                let frame_height = u32::from(u16::from_le_bytes([descriptor[6], descriptor[7]]));
                check_frame((width, height), (x, y), (frame_width, frame_height))?;
                if descriptor[8] & 0x18 != 0 {
                    return Err(corrupt("GIF frame uses reserved descriptor bits"));
                }
                let local_palette = descriptor[8] & 0x80 != 0;
                if local_palette {
                    skip(reader, gif_color_table_bytes(descriptor[8]))?;
                }
                if !global_palette && !local_palette {
                    return Err(corrupt("GIF frame has no color table"));
                }
                let code_size = byte(reader)?;
                if !(2..=8).contains(&code_size) {
                    return Err(corrupt("invalid GIF LZW code size"));
                }
                if !gif_sub_blocks(reader)? {
                    return Err(corrupt("GIF frame has no image data"));
                }
                frames = frames.checked_add(1).ok_or_else(|| corrupt("GIF frame count overflow"))?;
            }
            0x21 => gif_extension(reader)?,
            0x3b if frames != 0 => return Ok(Structure { width, height, frames }),
            0x3b => return Err(corrupt("GIF contains no complete frames")),
            _ => return Err(corrupt("invalid GIF block introducer")),
        }
    }
}

fn gif_extension(reader: &mut impl Read) -> Result<(), ProductRunnerError> {
    match byte(reader)? {
        0xf9 => {
            if byte(reader)? != 4 {
                return Err(corrupt("invalid GIF graphics-control extension"));
            }
            let control = bytes::<4>(reader)?;
            if control[0] & 0xe0 != 0 || (control[0] >> 2) & 0x07 > 3 {
                return Err(corrupt("GIF graphics control uses reserved fields"));
            }
            if byte(reader)? != 0 {
                return Err(corrupt("unterminated GIF graphics-control extension"));
            }
        }
        0x01 => gif_prefixed_extension(reader, 12)?,
        0xff => gif_prefixed_extension(reader, 11)?,
        0xfe => {
            let _ = gif_sub_blocks(reader)?;
        }
        _ => {
            let _ = gif_sub_blocks(reader)?;
        }
    }
    Ok(())
}

fn gif_prefixed_extension(
    reader: &mut impl Read,
    expected: u8,
) -> Result<(), ProductRunnerError> {
    if byte(reader)? != expected {
        return Err(corrupt("invalid GIF extension header"));
    }
    skip(reader, u64::from(expected))?;
    let _ = gif_sub_blocks(reader)?;
    Ok(())
}

fn gif_sub_blocks(reader: &mut impl Read) -> Result<bool, ProductRunnerError> {
    let mut any = false;
    loop {
        let length = byte(reader)?;
        if length == 0 {
            return Ok(any);
        }
        any = true;
        skip(reader, u64::from(length))?;
    }
}

fn gif_color_table_bytes(packed: u8) -> u64 {
    3_u64 * (1_u64 << (u32::from(packed & 0x07) + 1))
}

fn webp(reader: &mut impl Read, source_length: u64) -> Result<Structure, ProductRunnerError> {
    let header = bytes::<12>(reader)?;
    if header[..4] != *b"RIFF" || header[8..] != *b"WEBP" {
        return Err(corrupt("invalid WebP RIFF header"));
    }
    let declared = u64::from(u32::from_le_bytes(
        header[4..8].try_into().map_err(|_| corrupt("invalid WebP length"))?,
    ))
    .checked_add(8)
    .ok_or_else(|| corrupt("WebP length overflow"))?;
    if declared < 12 || declared > source_length {
        return Err(corrupt("truncated WebP RIFF container"));
    }
    let mut consumed = 12_u64;
    let mut canvas = None;
    let mut still = None;
    let mut extended_animation = false;
    let mut animation_control = false;
    let mut frames = 0_u32;
    let mut chunks = 0_u64;
    while consumed < declared {
        if declared - consumed < 8 {
            return Err(corrupt("truncated WebP chunk header"));
        }
        let chunk = bytes::<8>(reader)?;
        consumed += 8;
        let kind: [u8; 4] = chunk[..4].try_into().map_err(|_| corrupt("invalid WebP chunk type"))?;
        chunks = chunks.checked_add(1).ok_or_else(|| corrupt("WebP chunk count overflow"))?;
        let length = u64::from(u32::from_le_bytes(
            chunk[4..].try_into().map_err(|_| corrupt("invalid WebP chunk length"))?,
        ));
        let padded = length.checked_add(length & 1).ok_or_else(|| corrupt("WebP chunk length overflow"))?;
        if padded > declared - consumed {
            return Err(corrupt("WebP chunk exceeds its RIFF container"));
        }
        let mut capture = [0_u8; 16];
        let (retained, frame_pixels) = if kind == *b"ANMF" {
            if length < 16 {
                return Err(corrupt("truncated WebP animation frame"));
            }
            reader.read_exact(&mut capture).map_err(io_error)?;
            (16, Some(webp_frame_payload(reader, length - 16)?))
        } else {
            (read_capture(reader, length, &mut capture)?, None)
        };
        match &kind {
            b"VP8X" => {
                if chunks != 1 || canvas.is_some() || length != 10 || retained != 10 {
                    return Err(corrupt("invalid WebP extended header"));
                }
                if capture[0] & 0xc1 != 0 || capture[1..4] != [0, 0, 0] {
                    return Err(corrupt("invalid WebP extended-header flags"));
                }
                let width = le_u24(&capture[4..7])?.checked_add(1).ok_or_else(|| corrupt("WebP width overflow"))?;
                let height = le_u24(&capture[7..10])?.checked_add(1).ok_or_else(|| corrupt("WebP height overflow"))?;
                check_dimensions(width, height)?;
                canvas = Some((width, height));
                extended_animation = capture[0] & 0x02 != 0;
            }
            b"VP8 " => {
                let dimensions = vp8_dimensions(&capture[..retained])?;
                if still.replace(dimensions).is_some() {
                    return Err(corrupt("WebP has multiple top-level still images"));
                }
            }
            b"VP8L" => {
                let dimensions = vp8l_dimensions(&capture[..retained])?;
                if still.replace(dimensions).is_some() {
                    return Err(corrupt("WebP has multiple top-level still images"));
                }
            }
            b"ANIM" => {
                if !extended_animation || canvas.is_none() || length != 6 || animation_control {
                    return Err(corrupt("invalid WebP animation control"));
                }
                animation_control = true;
            }
            b"ANMF" => {
                let dimensions = canvas.ok_or_else(|| corrupt("WebP animation frame precedes canvas"))?;
                if !extended_animation || !animation_control || retained < 16 {
                    return Err(corrupt("truncated WebP animation frame"));
                }
                let x = le_u24(&capture[0..3])?.checked_mul(2).ok_or_else(|| corrupt("WebP frame x overflow"))?;
                let y = le_u24(&capture[3..6])?.checked_mul(2).ok_or_else(|| corrupt("WebP frame y overflow"))?;
                let width = le_u24(&capture[6..9])?.checked_add(1).ok_or_else(|| corrupt("WebP frame width overflow"))?;
                let height = le_u24(&capture[9..12])?.checked_add(1).ok_or_else(|| corrupt("WebP frame height overflow"))?;
                check_frame(dimensions, (x, y), (width, height))?;
                if capture[15] & 0xfc != 0 {
                    return Err(corrupt("WebP animation frame uses reserved flags"));
                }
                if frame_pixels != Some((width, height)) {
                    return Err(corrupt("WebP frame dimensions differ from its pixel chunk"));
                }
                frames = frames.checked_add(1).ok_or_else(|| corrupt("WebP frame count overflow"))?;
            }
            _ => {}
        }
        if length & 1 != 0 {
            skip(reader, 1)?;
        }
        consumed = consumed.checked_add(padded).ok_or_else(|| corrupt("WebP length overflow"))?;
    }
    if consumed != declared {
        return Err(corrupt("WebP chunks do not fill their RIFF container"));
    }
    match (extended_animation, animation_control, frames, canvas, still) {
        (true, true, frames, Some((width, height)), None) if frames != 0 => {
            Ok(Structure { width, height, frames })
        }
        (false, false, 0, Some(canvas), Some(still)) if canvas == still => {
            Ok(Structure { width: canvas.0, height: canvas.1, frames: 1 })
        }
        (false, false, 0, None, Some((width, height))) => {
            Ok(Structure { width, height, frames: 1 })
        }
        _ => Err(corrupt("inconsistent WebP image and animation chunks")),
    }
}

fn webp_frame_payload(
    reader: &mut impl Read,
    length: u64,
) -> Result<(u32, u32), ProductRunnerError> {
    let mut consumed = 0_u64;
    let mut pixels = None;
    while consumed < length {
        if length - consumed < 8 {
            return Err(corrupt("truncated WebP frame chunk header"));
        }
        let header = bytes::<8>(reader)?;
        consumed += 8;
        let kind: [u8; 4] = header[..4]
            .try_into()
            .map_err(|_| corrupt("invalid WebP frame chunk type"))?;
        let chunk_length = u64::from(u32::from_le_bytes(
            header[4..]
                .try_into()
                .map_err(|_| corrupt("invalid WebP frame chunk length"))?,
        ));
        let padded = chunk_length
            .checked_add(chunk_length & 1)
            .ok_or_else(|| corrupt("WebP frame chunk length overflow"))?;
        if padded > length - consumed {
            return Err(corrupt("WebP frame chunk exceeds its animation frame"));
        }
        let mut capture = [0_u8; 10];
        let retained = read_capture(reader, chunk_length, &mut capture)?;
        let dimensions = match &kind {
            b"VP8 " => Some(vp8_dimensions(&capture[..retained])?),
            b"VP8L" => Some(vp8l_dimensions(&capture[..retained])?),
            _ => None,
        };
        if let Some(dimensions) = dimensions {
            if pixels.replace(dimensions).is_some() {
                return Err(corrupt("WebP animation frame has multiple pixel chunks"));
            }
        }
        if chunk_length & 1 != 0 {
            skip(reader, 1)?;
        }
        consumed = consumed
            .checked_add(padded)
            .ok_or_else(|| corrupt("WebP frame length overflow"))?;
    }
    pixels.ok_or_else(|| corrupt("WebP animation frame has no pixel chunk"))
}

fn vp8_dimensions(bytes: &[u8]) -> Result<(u32, u32), ProductRunnerError> {
    if bytes.len() < 10
        || bytes[0] & 0x01 != 0
        || bytes[0] & 0x10 == 0
        || bytes[3..6] != [0x9d, 0x01, 0x2a]
    {
        return Err(corrupt("invalid WebP VP8 frame header"));
    }
    let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]) & 0x3fff);
    let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]) & 0x3fff);
    check_dimensions(width, height)?;
    Ok((width, height))
}

fn vp8l_dimensions(bytes: &[u8]) -> Result<(u32, u32), ProductRunnerError> {
    if bytes.len() < 5 || bytes[0] != 0x2f {
        return Err(corrupt("invalid WebP VP8L frame header"));
    }
    let bits = u32::from_le_bytes(bytes[1..5].try_into().map_err(|_| corrupt("invalid WebP VP8L dimensions"))?);
    if bits >> 29 != 0 {
        return Err(corrupt("unsupported WebP VP8L bitstream version"));
    }
    let width = (bits & 0x3fff) + 1;
    let height = ((bits >> 14) & 0x3fff) + 1;
    check_dimensions(width, height)?;
    Ok((width, height))
}

fn le_u24(bytes: &[u8]) -> Result<u32, ProductRunnerError> {
    if bytes.len() != 3 {
        return Err(corrupt("invalid WebP 24-bit integer"));
    }
    Ok(u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16))
}

fn check_dimensions(width: u32, height: u32) -> Result<(), ProductRunnerError> {
    if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
        return Err(corrupt("image canvas is empty or exceeds its format representation"));
    }
    Ok(())
}

fn check_frame(
    canvas: (u32, u32),
    origin: (u32, u32),
    size: (u32, u32),
) -> Result<(), ProductRunnerError> {
    check_dimensions(size.0, size.1)?;
    if origin.0.checked_add(size.0).is_none_or(|edge| edge > canvas.0)
        || origin.1.checked_add(size.1).is_none_or(|edge| edge > canvas.1)
    {
        return Err(corrupt("image frame exceeds its canvas"));
    }
    Ok(())
}

fn read_capture(
    reader: &mut impl Read,
    length: u64,
    capture: &mut [u8],
) -> Result<usize, ProductRunnerError> {
    let mut remaining = length;
    let mut copied = 0_usize;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| corrupt("image chunk length is not representable"))?;
        reader.read_exact(&mut buffer[..count]).map_err(io_error)?;
        let retained = (capture.len() - copied).min(count);
        capture[copied..copied + retained].copy_from_slice(&buffer[..retained]);
        copied += retained;
        remaining -= u64::try_from(count).map_err(|_| corrupt("image chunk length overflow"))?;
    }
    Ok(copied)
}

fn skip(reader: &mut impl Read, length: u64) -> Result<(), ProductRunnerError> {
    let mut sink = [0_u8; 64 * 1024];
    let mut remaining = length;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(sink.len() as u64))
            .map_err(|_| corrupt("image structure length is not representable"))?;
        reader.read_exact(&mut sink[..count]).map_err(io_error)?;
        remaining -= u64::try_from(count).map_err(|_| corrupt("image structure length overflow"))?;
    }
    Ok(())
}

fn bytes<const N: usize>(reader: &mut impl Read) -> Result<[u8; N], ProductRunnerError> {
    let mut bytes = [0_u8; N];
    reader.read_exact(&mut bytes).map_err(io_error)?;
    Ok(bytes)
}

fn byte(reader: &mut impl Read) -> Result<u8, ProductRunnerError> {
    Ok(bytes::<1>(reader)?[0])
}

const fn crc32_table() -> [u32; 256] {
    let mut table = [0_u32; 256];
    let mut index = 0_usize;
    while index < table.len() {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 0 { value >> 1 } else { 0xedb8_8320 ^ (value >> 1) };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

const CRC32_TABLE: [u32; 256] = crc32_table();

const fn crc32_start() -> u32 {
    u32::MAX
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        let index = usize::from((crc as u8) ^ byte);
        crc = CRC32_TABLE[index] ^ (crc >> 8);
    }
    crc
}

const fn crc32_finish(crc: u32) -> u32 {
    !crc
}

fn io_error(error: std::io::Error) -> ProductRunnerError {
    corrupt(format!("truncated image container: {error}"))
}

fn corrupt(detail: impl Into<String>) -> ProductRunnerError {
    invalid(format!("image container inspection failed: {}", detail.into()))
}
