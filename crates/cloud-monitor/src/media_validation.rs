//! Bounded image decoding shared by live fetch and staging receipt paths.

use image::{ImageFormat, ImageReader, Limits};
use std::io::Cursor;

/// These limits are intentionally lower than the 128 MiB transport cap. They
/// bound decoder dimensions and allocations before a receipt can be issued.
pub const MAX_IMAGE_WIDTH: u32 = 20_000;
pub const MAX_IMAGE_HEIGHT: u32 = 20_000;
pub const MAX_IMAGE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

fn format(format: &str) -> Option<ImageFormat> {
    match format {
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        _ => None,
    }
}

/// Decode the complete image stream using the descriptor's declared format.
/// The original bytes remain unchanged for no-op/animated formats, but they
/// must still be decodable before downstream staging treats them as success.
pub(crate) fn validate(format_name: &str, bytes: &[u8]) -> Result<(), String> {
    decode(format_name, bytes).map(|_| ())
}

pub(crate) fn decode(format_name: &str, bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let format = format(format_name).ok_or("UNSUPPORTED_IMAGE_FORMAT")?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC_BYTES);
    reader.limits(limits);
    reader.decode().map_err(|_| {
        match format_name {
            "gif" => "IMAGE_GIF_DECODE_FAILED",
            "webp" => "IMAGE_WEBP_DECODE_FAILED",
            "jpg" | "jpeg" => "IMAGE_JPEG_DECODE_FAILED",
            "png" => "IMAGE_PNG_DECODE_FAILED",
            _ => "UNSUPPORTED_IMAGE_FORMAT",
        }
        .to_owned()
    })
}
