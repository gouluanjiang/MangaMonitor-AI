//! A6.14B JM pixel transform copied from the pinned upstream behavior.
//!
//! Descriptor-bound WEBP is restored and encoded directly to the selected
//! legacy WEBP or JPEG format. GIF remains byte-exact.
//! The transport remains private and authorization/staging authority is owned by
//! the surrounding A6.10/A6.12 execution chain.

use image::{DynamicImage, ImageFormat, RgbImage};
use std::io::Cursor;

fn stitch_rgb(src_img: &RgbImage, block_num: u32) -> Result<RgbImage, String> {
    if block_num == 0 {
        return Ok(src_img.clone());
    }

    let (width, height) = src_img.dimensions();
    if width == 0 || height == 0 {
        return Err("JM_MEDIA_IMAGE_EMPTY".into());
    }

    let mut stitched_img = image::ImageBuffer::new(width, height);
    let remainder_height = height % block_num;
    for i in 0..block_num {
        let mut block_height = height / block_num;
        let src_img_y_start = height
            .checked_sub(block_height.saturating_mul(i + 1))
            .and_then(|value| value.checked_sub(remainder_height))
            .ok_or("JM_MEDIA_BLOCK_GEOMETRY_INVALID")?;
        let mut dst_img_y_start = block_height.saturating_mul(i);
        if i == 0 {
            block_height = block_height
                .checked_add(remainder_height)
                .ok_or("JM_MEDIA_BLOCK_GEOMETRY_INVALID")?;
        } else {
            dst_img_y_start = dst_img_y_start
                .checked_add(remainder_height)
                .ok_or("JM_MEDIA_BLOCK_GEOMETRY_INVALID")?;
        }

        for y in 0..block_height {
            let src_y = src_img_y_start
                .checked_add(y)
                .ok_or("JM_MEDIA_BLOCK_GEOMETRY_INVALID")?;
            let dst_y = dst_img_y_start
                .checked_add(y)
                .ok_or("JM_MEDIA_BLOCK_GEOMETRY_INVALID")?;
            if src_y >= height || dst_y >= height {
                return Err("JM_MEDIA_BLOCK_GEOMETRY_INVALID".into());
            }
            for x in 0..width {
                stitched_img.put_pixel(x, dst_y, *src_img.get_pixel(x, src_y));
            }
        }
    }

    Ok(stitched_img)
}

pub(crate) fn apply(
    source_format: &str,
    transform: &str,
    transform_parameter: u64,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    match (source_format, transform, transform_parameter) {
        ("webp", "JM_SCRAMBLE_BLOCKS_JPEG", parameter) => {
            let block_num = u32::try_from(parameter).map_err(|_| "JM_MEDIA_BLOCK_COUNT_INVALID")?;
            let src = crate::media_validation::decode("webp", &bytes)
                .map_err(|_| "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED")?
                .into_rgb8();
            let dst = if block_num == 0 {
                src
            } else {
                stitch_rgb(&src, block_num)?
            };
            let mut encoded = Cursor::new(Vec::new());
            // Same default JPEG encoder/quality as the pinned downloader. No
            // intermediate lossless WEBP encode and no additional CPU workers.
            DynamicImage::ImageRgb8(dst)
                .write_to(&mut encoded, ImageFormat::Jpeg)
                .map_err(|_| "JM_MEDIA_JPEG_ENCODE_FAILED")?;
            Ok(encoded.into_inner())
        }
        ("gif", "NONE", 0) | ("webp", "JM_SCRAMBLE_BLOCKS", 0) => Ok(bytes),
        ("webp", "JM_SCRAMBLE_BLOCKS", parameter) => {
            let block_num = u32::try_from(parameter).map_err(|_| "JM_MEDIA_BLOCK_COUNT_INVALID")?;
            if block_num == 0 {
                return Err("JM_MEDIA_BLOCK_COUNT_INVALID".into());
            }

            let src_img = image::load_from_memory_with_format(&bytes, ImageFormat::WebP)
                .map_err(|_| "JM_MEDIA_WEBP_DECODE_FAILED")?
                .to_rgb8();
            let dst_img = stitch_rgb(&src_img, block_num)?;

            let mut cursor = Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(dst_img)
                .write_to(&mut cursor, ImageFormat::WebP)
                .map_err(|_| "JM_MEDIA_WEBP_ENCODE_FAILED")?;
            let encoded = cursor.into_inner();
            if encoded.len() < 12 || &encoded[..4] != b"RIFF" || &encoded[8..12] != b"WEBP" {
                return Err("JM_MEDIA_WEBP_ENCODE_INVALID".into());
            }
            Ok(encoded)
        }
        _ => Err("LIVE_MEDIA_FETCH_TRANSFORM_NOT_SUPPORTED".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn row_image(values: &[u8]) -> RgbImage {
        let mut image = RgbImage::new(1, u32::try_from(values.len()).unwrap());
        for (y, value) in values.iter().copied().enumerate() {
            image.put_pixel(0, u32::try_from(y).unwrap(), Rgb([value, value, value]));
        }
        image
    }

    fn rows(image: &RgbImage) -> Vec<u8> {
        (0..image.height())
            .map(|y| image.get_pixel(0, y).0[0])
            .collect()
    }

    #[test]
    fn pinned_two_block_order_moves_bottom_half_first() {
        let source = row_image(&[10, 20, 30, 40]);
        let stitched = stitch_rgb(&source, 2).unwrap();
        assert_eq!(rows(&stitched), vec![30, 40, 10, 20]);
    }

    #[test]
    fn pinned_remainder_rows_belong_to_first_output_block() {
        let source = row_image(&[10, 20, 30, 40, 50]);
        let stitched = stitch_rgb(&source, 2).unwrap();
        assert_eq!(rows(&stitched), vec![30, 40, 50, 10, 20]);
    }

    #[test]
    fn zero_block_transform_remains_exact_byte_noop() {
        let bytes = b"RIFF1234WEBPfixture".to_vec();
        assert_eq!(
            apply("webp", "JM_SCRAMBLE_BLOCKS", 0, bytes.clone()).unwrap(),
            bytes
        );
    }

    #[test]
    fn unsupported_transform_fails_closed() {
        assert_eq!(
            apply("webp", "NONE", 1, vec![1]).unwrap_err(),
            "LIVE_MEDIA_FETCH_TRANSFORM_NOT_SUPPORTED"
        );
    }
}
