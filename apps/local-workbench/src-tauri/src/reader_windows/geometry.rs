use workbench_storage::{ReaderWindowSize, StoreError};

#[derive(Clone, Copy)]
pub(super) struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}
pub(super) struct Placement {
    pub width: f64,
    pub height: f64,
    pub x: i32,
    pub y: i32,
}

pub(super) fn placement(
    area: Area,
    saved: Option<ReaderWindowSize>,
    ordinal: usize,
) -> Result<Placement, StoreError> {
    if area.width == 0 || area.height == 0 || !area.scale.is_finite() || area.scale <= 0.0 {
        return Err(StoreError {
            code: "READER_WINDOW_MONITOR_UNAVAILABLE",
        });
    }
    // Leave decoration space before creation; actual outer bounds are measured
    // and corrected while hidden before the user ever sees the window.
    let available_w = (f64::from(area.width) / area.scale - 32.0).max(1.0);
    let available_h = (f64::from(area.height) / area.scale - 80.0).max(1.0);
    let desired = saved.unwrap_or(ReaderWindowSize {
        width: 874.0 * 1206.0 / 2622.0,
        height: 874.0,
    });
    if !desired.width.is_finite()
        || !desired.height.is_finite()
        || desired.width <= 0.0
        || desired.height <= 0.0
    {
        return Err(StoreError {
            code: "READER_WINDOW_SIZE_INVALID",
        });
    }
    let shrink = (available_w / desired.width)
        .min(available_h / desired.height)
        .min(1.0);
    let width = (desired.width * shrink).max(1.0);
    let height = (desired.height * shrink).max(1.0);
    let outer_w = (width * area.scale + 16.0 * area.scale)
        .ceil()
        .min(f64::from(area.width)) as u32;
    let outer_h = (height * area.scale + 48.0 * area.scale)
        .ceil()
        .min(f64::from(area.height)) as u32;
    let (x, y) = contained_position(area, outer_w, outer_h, ordinal)?;
    Ok(Placement {
        width,
        height,
        x,
        y,
    })
}

pub(super) fn contained_position(
    area: Area,
    outer_w: u32,
    outer_h: u32,
    ordinal: usize,
) -> Result<(i32, i32), StoreError> {
    if outer_w > area.width || outer_h > area.height {
        return Err(StoreError {
            code: "READER_WINDOW_SIZE_INVALID",
        });
    }
    let step = (28.0 * area.scale).round().max(1.0) as u64;
    let offset = (ordinal as u64 % 12) * step;
    let x = i64::from(area.x)
        + i64::try_from(offset % (u64::from(area.width - outer_w) + 1)).unwrap_or(0);
    let y = i64::from(area.y)
        + i64::try_from(offset % (u64::from(area.height - outer_h) + 1)).unwrap_or(0);
    Ok((
        i32::try_from(x).map_err(|_| StoreError {
            code: "READER_WINDOW_SIZE_INVALID",
        })?,
        i32::try_from(y).map_err(|_| StoreError {
            code: "READER_WINDOW_SIZE_INVALID",
        })?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phone_ratio_and_saved_sizes_fit_portrait_landscape_negative_origin_and_high_dpi() {
        for area in [
            Area {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
                scale: 1.0,
            },
            Area {
                x: -2560,
                y: 40,
                width: 2560,
                height: 1360,
                scale: 1.5,
            },
            Area {
                x: 100,
                y: -1080,
                width: 600,
                height: 950,
                scale: 2.0,
            },
            Area {
                x: 0,
                y: 0,
                width: 250,
                height: 280,
                scale: 1.25,
            },
        ] {
            for ordinal in 0..30 {
                for size in [
                    None,
                    Some(ReaderWindowSize {
                        width: 4000.0,
                        height: 3000.0,
                    }),
                ] {
                    let p = placement(area, size, ordinal).unwrap();
                    let width = (p.width * area.scale).ceil() as u32;
                    let height = (p.height * area.scale).ceil() as u32;
                    assert!(p.x >= area.x && p.y >= area.y);
                    assert!(
                        i64::from(p.x) + i64::from(width)
                            <= i64::from(area.x) + i64::from(area.width)
                    );
                    assert!(
                        i64::from(p.y) + i64::from(height)
                            <= i64::from(area.y) + i64::from(area.height)
                    );
                    if size.is_none() {
                        assert!((p.width / p.height - 1206.0 / 2622.0).abs() < 1e-9);
                    }
                }
            }
        }
        assert!(placement(
            Area {
                x: 0,
                y: 0,
                width: 0,
                height: 500,
                scale: 1.0
            },
            None,
            0
        )
        .is_err());
    }
}
