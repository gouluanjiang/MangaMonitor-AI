use super::{ImageDescriptor, ReaderImage};
use crate::{jm_media_transform, media_validation};
use std::io::Cursor;

const MAX_READER_BYTES: usize = 32 * 1024 * 1024;
const MAX_READER_PIXELS: u64 = 32_000_000;

pub(super) fn decode(descriptor: ImageDescriptor, bytes: Vec<u8>) -> Result<ReaderImage, String> {
    if bytes.is_empty() || bytes.len() > MAX_READER_BYTES {
        return Err("READER_IMAGE_LIMIT".into());
    }
    let source_format = match &descriptor {
        ImageDescriptor::Jm { item, .. } => item.source_format.as_str(),
        ImageDescriptor::Pica(_) => {
            media_validation::detected_format(&bytes).map_err(|_| "READER_IMAGE_INVALID")?
        }
    };
    let format = match source_format {
        "webp" => ::image::ImageFormat::WebP,
        "gif" => ::image::ImageFormat::Gif,
        "jpg" | "jpeg" => ::image::ImageFormat::Jpeg,
        "png" => ::image::ImageFormat::Png,
        _ => return Err("READER_IMAGE_INVALID".into()),
    };
    let (width, height) = ::image::ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "READER_IMAGE_INVALID")?;
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || u64::from(width) * u64::from(height) > MAX_READER_PIXELS
    {
        return Err("READER_IMAGE_LIMIT".into());
    }
    let image = match descriptor {
        ImageDescriptor::Jm { item, .. } if item.source_format == "webp" => {
            // Same pinned block restoration and encoder as accepted downloads,
            // without staging or a second decoding pass just to get dimensions.
            let (bytes, width, height) =
                jm_media_transform::jpeg_with_dimensions(item.block_num, &bytes)?;
            ReaderImage {
                bytes,
                mime: "image/jpeg",
                width,
                height,
            }
        }
        ImageDescriptor::Jm { item, .. } if item.source_format == "gif" && item.block_num == 0 => {
            original("gif", bytes)?
        }
        ImageDescriptor::Pica(_) => {
            // Preserve the accepted Pica fix: URL/MIME may say JPG while the
            // response is WebP. Detection alone is not success; bounded decode
            // validates it, then the original bytes (including animation) remain.
            let format =
                media_validation::detected_format(&bytes).map_err(|_| "READER_IMAGE_INVALID")?;
            original(format, bytes)?
        }
        _ => return Err("READER_IMAGE_INVALID".into()),
    };
    if image.bytes.len() > MAX_READER_BYTES {
        return Err("READER_IMAGE_LIMIT".into());
    }
    Ok(image)
}

fn original(format: &str, bytes: Vec<u8>) -> Result<ReaderImage, String> {
    let decoded = media_validation::decode(format, &bytes).map_err(|_| "READER_IMAGE_INVALID")?;
    let mime = match format {
        "gif" => "image/gif",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => return Err("READER_IMAGE_INVALID".into()),
    };
    Ok(ReaderImage {
        width: decoded.width(),
        height: decoded.height(),
        bytes,
        mime,
    })
}
