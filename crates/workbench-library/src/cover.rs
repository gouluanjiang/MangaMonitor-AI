use crate::{archive::MAX_IMAGE_BYTES, error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

pub(crate) fn thumbnail(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(error("LIBRARY_COVER_LIMIT"));
    }
    let format = image::guess_format(bytes).map_err(|_| error("LIBRARY_COVER_INVALID"))?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif
    ) {
        return Err(error("LIBRARY_COVER_INVALID"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(192 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| error("LIBRARY_COVER_INVALID"))?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 24_000_000
    {
        return Err(error("LIBRARY_COVER_LIMIT"));
    }
    let decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|_| error("LIBRARY_COVER_INVALID"))?;
    let small = decoded.thumbnail(512, 512).to_rgb8();
    for quality in [85, 65, 45] {
        let mut output = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality)
            .encode_image(&small)
            .map_err(|_| error("LIBRARY_COVER_INVALID"))?;
        if output.len() <= 256 * 1024 {
            return Ok(format!(
                "data:image/jpeg;base64,{}",
                STANDARD.encode(output)
            ));
        }
    }
    Err(error("LIBRARY_COVER_LIMIT"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_are_checked_before_full_decode_and_output_is_bounded_jpeg() {
        let mut oversized = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(9000, 1)
            .write_to(&mut oversized, ImageFormat::Png)
            .unwrap();
        assert!(thumbnail(oversized.get_ref()).is_err());
        let mut input = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1200, 900)
            .write_to(&mut input, ImageFormat::Png)
            .unwrap();
        let url = thumbnail(input.get_ref()).unwrap();
        let bytes = STANDARD
            .decode(url.strip_prefix("data:image/jpeg;base64,").unwrap())
            .unwrap();
        assert!(bytes.len() <= 256 * 1024);
        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).unwrap();
        assert!(image.width() <= 512 && image.height() <= 512);
    }
}
