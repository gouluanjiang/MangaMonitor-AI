use crate::{protocol::error, SourceResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

pub(crate) const MAX_COVER_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 256 * 1024;

pub(crate) fn data_url(bytes: &[u8]) -> SourceResult<String> {
    let decoded = decode_cover(bytes)?;
    let output = encode_thumbnail(&decoded)?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(output)
    ))
}

fn decode_cover(bytes: &[u8]) -> SourceResult<image::DynamicImage> {
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
    image::DynamicImage::from_decoder(decoder).map_err(|_| error("SOURCE_COVER_INVALID"))
}

fn encode_thumbnail(decoded: &image::DynamicImage) -> SourceResult<Vec<u8>> {
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
    Ok(output)
}

#[cfg(test)]
mod profiling {
    use super::*;
    use std::{hint::black_box, time::Instant};

    /// Separate release-mode diagnostic: synthetic data, no source or filesystem access.
    #[test]
    #[ignore = "run once in the desktop CI release profile, not in the ordinary suite"]
    fn cover_pipeline_profile() {
        fn measure<T>(mut run: impl FnMut() -> T) -> (u128, u128) {
            black_box(run());
            let mut micros: Vec<_> = (0..12)
                .map(|_| {
                    let start = Instant::now();
                    black_box(run());
                    start.elapsed().as_micros()
                })
                .collect();
            micros.sort_unstable();
            (micros[6], micros[11])
        }
        for (width, height) in [(512, 384), (1600, 2000)] {
            let fixture = image::RgbImage::from_fn(width, height, |x, y| {
                image::Rgb([
                    ((x / 8 + y / 7) % 256) as u8,
                    ((x / 13) % 256) as u8,
                    ((y / 11) % 256) as u8,
                ])
            });
            let mut input = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut input, 85)
                .encode_image(&fixture)
                .unwrap();
            assert!(input.len() <= MAX_COVER_BYTES);
            let decoded = decode_cover(&input).unwrap();
            let jpeg = encode_thumbnail(&decoded).unwrap();
            let encoded = STANDARD.encode(&jpeg);
            let (total, total_tail) = measure(|| data_url(&input).unwrap());
            let (decode, _) = measure(|| decode_cover(&input).unwrap());
            let (resize_encode, _) = measure(|| encode_thumbnail(&decoded).unwrap());
            let (base64, _) = measure(|| STANDARD.decode(STANDARD.encode(&jpeg)).unwrap());
            let (validate_decode, _) = measure(|| {
                let bytes = STANDARD.decode(&encoded).unwrap();
                decode_cover(&bytes).unwrap()
            });
            println!(
                "COVER_PIPELINE_PROFILE {}",
                serde_json::json!({
                    "synthetic": true, "release_profile": !cfg!(debug_assertions),
                    "width": width, "height": height, "input_bytes": input.len(), "thumbnail_bytes": jpeg.len(),
                    "samples": 12, "unit": "microseconds", "pipeline_median": total,
                    "pipeline_max": total_tail, "source_decode_median": decode,
                    "resize_jpeg_median": resize_encode, "base64_roundtrip_median": base64,
                    "normalized_decode_median": validate_decode,
                    "excludes": ["network", "queue_wait", "zip_io", "webview_ipc", "browser_decode"]
                })
            );
        }
    }
}
