use crate::{store::read_regular_bounded, Result, StoreError};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use serde::Serialize;
use std::{io::Cursor, path::Path};

pub const MAX_BACKGROUND_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = MAX_BACKGROUND_BYTES.div_ceil(3) * 4;
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 24_000_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundSelection {
    pub background_image: String,
    pub background_name: String,
}

fn format_and_mime(bytes: &[u8]) -> Result<(ImageFormat, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok((ImageFormat::Png, "image/png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok((ImageFormat::Jpeg, "image/jpeg"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Ok((ImageFormat::WebP, "image/webp"))
    } else {
        Err(StoreError::new("BACKGROUND_INVALID"))
    }
}

fn validate_bytes(bytes: &[u8]) -> Result<&'static str> {
    if bytes.len() > MAX_BACKGROUND_BYTES {
        return Err(StoreError::new("BACKGROUND_TOO_LARGE"));
    }
    let (format, mime) = format_and_mime(bytes)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(192 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|error| match error {
        image::ImageError::Limits(_) => StoreError::new("BACKGROUND_DIMENSIONS"),
        _ => StoreError::new("BACKGROUND_INVALID"),
    })?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(StoreError::new("BACKGROUND_DIMENSIONS"));
    }
    // Dimension and allocation bounds apply before decompression allocates its output.
    image::DynamicImage::from_decoder(decoder)
        .map_err(|_| StoreError::new("BACKGROUND_INVALID"))?;
    Ok(mime)
}

pub(crate) fn validate_data_url(value: &str) -> Result<()> {
    let (header, encoded) = value
        .split_once(',')
        .ok_or(StoreError::new("BACKGROUND_INVALID"))?;
    let declared = match header {
        "data:image/png;base64" => "image/png",
        "data:image/jpeg;base64" => "image/jpeg",
        "data:image/webp;base64" => "image/webp",
        _ => return Err(StoreError::new("BACKGROUND_INVALID")),
    };
    if encoded.len() > MAX_ENCODED_BYTES {
        return Err(StoreError::new("BACKGROUND_TOO_LARGE"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| StoreError::new("BACKGROUND_INVALID"))?;
    if validate_bytes(&bytes)? != declared {
        return Err(StoreError::new("BACKGROUND_INVALID"));
    }
    Ok(())
}

fn trim_display_name(value: &str) -> &str {
    // Cc characters have already been removed. Include the ECMAScript trim BOM.
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

/// Only the native file picker may supply this path. It is not an IPC argument.
/// The original path is never persisted or included in the returned object.
pub fn background_from_path(path: &Path) -> Result<BackgroundSelection> {
    let bytes =
        read_regular_bounded(path, MAX_BACKGROUND_BYTES).map_err(|error| match error.code {
            "DOCUMENT_TOO_LARGE" => StoreError::new("BACKGROUND_TOO_LARGE"),
            "UNSAFE_PATH" => error,
            _ => StoreError::new("BACKGROUND_READ_FAILED"),
        })?;
    let mime = validate_bytes(&bytes)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StoreError::new("BACKGROUND_INVALID"))?;
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .collect();
    let mut display_name = String::new();
    let mut units = 0;
    for character in trim_display_name(&cleaned).chars() {
        units += character.len_utf16();
        if units > 180 {
            break;
        }
        display_name.push(character);
    }
    let display_name = trim_display_name(&display_name);
    Ok(BackgroundSelection {
        background_image: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        background_name: if display_name.is_empty() {
            "background"
        } else {
            display_name
        }
        .to_owned(),
    })
}
