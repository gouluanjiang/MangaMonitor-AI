use crate::{protocol::error, SourceResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

pub(crate) const MAX_COVER_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 256 * 1024;

pub(crate) fn data_url(bytes: &[u8]) -> SourceResult<String> {
    if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
        return Err(error("SOURCE_COVER_INVALID"));
    }
    let format = image::guess_format(bytes).map_err(|_| error("SOURCE_COVER_INVALID"))?;
    match format {
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif => (),
        _ => return Err(error("SOURCE_COVER_INVALID")),
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| error("SOURCE_COVER_INVALID"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_000_000 {
        return Err(error("SOURCE_COVER_INVALID"));
    }
    let decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|_| error("SOURCE_COVER_INVALID"))?;
    // The renderer receives one bounded static raster, including for animated
    // inputs. No remote active format or unbounded animation is forwarded.
    let raster = decoded.thumbnail(512, 512).to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85)
        .encode_image(&raster)
        .map_err(|_| error("SOURCE_COVER_INVALID"))?;
    if output.len() > MAX_OUTPUT_BYTES {
        return Err(error("SOURCE_COVER_INVALID"));
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(output)
    ))
}
