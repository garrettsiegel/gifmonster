use crate::config::EncodeConfig;
use crate::types::{EncodeStats, IndexedFrame, Palette, ProgressReporter, RgbaFrame};
use anyhow::{bail, Context, Result};
use gif::{DisposalMethod, Encoder, Frame, Repeat};
use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub fn encode_indexed_frames(
    output: &Path,
    indexed_frames: &[IndexedFrame],
    source_frames: &[RgbaFrame],
    config: &EncodeConfig,
    progress: Option<&dyn ProgressReporter>,
) -> Result<EncodeStats> {
    if indexed_frames.is_empty() {
        bail!("no indexed frames to encode");
    }

    if indexed_frames.len() != source_frames.len() {
        bail!(
            "indexed/source frame count mismatch: {} vs {}",
            indexed_frames.len(),
            source_frames.len()
        );
    }

    let width = indexed_frames[0].width;
    let height = indexed_frames[0].height;

    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        bail!(
            "GIF dimensions exceed format limits: {}x{} (max {}x{})",
            width,
            height,
            u16::MAX,
            u16::MAX
        );
    }

    let file = File::create(output)
        .with_context(|| format!("failed to create output file {}", output.display()))?;
    let mut writer = BufWriter::new(file);

    let mut encoder =
        Encoder::new(&mut writer, width as u16, height as u16, &[]).context("failed to create GIF encoder")?;
    encoder
        .set_repeat(Repeat::Infinite)
        .context("failed to set GIF repeat")?;

    for (frame_index, indexed) in indexed_frames.iter().enumerate() {
        if indexed.width != width || indexed.height != height {
            bail!("frame {} has mismatched dimensions", frame_index);
        }

        let expected_pixels = indexed.width as usize * indexed.height as usize;
        if indexed.indices.len() != expected_pixels {
            bail!(
                "frame {} has invalid index buffer length: expected {}, got {}",
                frame_index,
                expected_pixels,
                indexed.indices.len()
            );
        }

        let mut palette = indexed.palette.clone();
        let mut indices = indexed.indices.clone();
        let mut transparent_index = None;
        let mut disposal = DisposalMethod::Any;

        if config.transparency_optimization && frame_index > 0 {
            let tolerance = transparency_tolerance_from_quality(config.quality);
            transparent_index = apply_transparency_optimization(
                &mut palette,
                &mut indices,
                &source_frames[frame_index - 1],
                &source_frames[frame_index],
                tolerance,
            );
            if transparent_index.is_some() {
                disposal = DisposalMethod::Keep;
            }
        }

        let cropped = crop_to_changed_region(indexed.width, indexed.height, &indices, transparent_index)?;

        let palette_bytes = flatten_palette_for_gif(&palette);

        let frame = Frame {
            left: cropped.left,
            top: cropped.top,
            width: cropped.width,
            height: cropped.height,
            delay: indexed.delay_cs,
            dispose: disposal,
            transparent: transparent_index,
            palette: Some(palette_bytes),
            buffer: Cow::Owned(cropped.indices),
            ..Frame::default()
        };

        encoder
            .write_frame(&frame)
            .with_context(|| format!("failed to write frame {}", frame_index))?;

        if let Some(reporter) = progress {
            reporter.inc(1);
        }
    }

    drop(encoder);
    writer.flush().context("failed to flush GIF output")?;

    let metadata = std::fs::metadata(output)
        .with_context(|| format!("failed to stat output file {}", output.display()))?;

    let duration_ms = indexed_frames
        .iter()
        .map(|frame| frame.delay_cs as u64 * 10)
        .sum();

    Ok(EncodeStats {
        file_size_bytes: metadata.len(),
        frame_count: indexed_frames.len(),
        duration_ms,
        width,
        height,
    })
}

fn apply_transparency_optimization(
    palette: &mut Palette,
    indices: &mut [u8],
    previous: &RgbaFrame,
    current: &RgbaFrame,
    tolerance: u8,
) -> Option<u8> {
    if previous.width != current.width || previous.height != current.height {
        return None;
    }

    let pixel_count = current.pixel_count();
    if indices.len() != pixel_count {
        return None;
    }

    let mut unchanged_count = 0usize;

    for (prev, curr) in previous
        .pixels
        .chunks_exact(4)
        .zip(current.pixels.chunks_exact(4))
    {
        if pixels_similar(prev, curr, tolerance) {
            unchanged_count += 1;
        }
    }

    if unchanged_count == 0 {
        return None;
    }

    let transparent_index = if palette.len() < 256 {
        palette.push([0, 0, 0]);
        (palette.len() - 1) as u8
    } else {
        let victim = least_used_palette_index(indices, palette.len());
        let replacement = nearest_palette_neighbor(palette, victim).unwrap_or(victim);

        for index in indices.iter_mut() {
            if *index as usize == victim {
                *index = replacement as u8;
            }
        }

        palette[victim] = [0, 0, 0];
        victim as u8
    };

    for ((prev, curr), index) in previous
        .pixels
        .chunks_exact(4)
        .zip(current.pixels.chunks_exact(4))
        .zip(indices.iter_mut())
    {
        if pixels_similar(prev, curr, tolerance) {
            *index = transparent_index;
        }
    }

    Some(transparent_index)
}

#[derive(Debug)]
struct CroppedFrame {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    indices: Vec<u8>,
}

fn crop_to_changed_region(
    frame_width: u32,
    frame_height: u32,
    indices: &[u8],
    transparent_index: Option<u8>,
) -> Result<CroppedFrame> {
    let full_width = frame_width as usize;
    let full_height = frame_height as usize;

    if indices.len() != full_width * full_height {
        bail!(
            "cannot crop frame: invalid index buffer length {} for {}x{}",
            indices.len(),
            frame_width,
            frame_height
        );
    }

    let Some(transparent) = transparent_index else {
        return Ok(CroppedFrame {
            left: 0,
            top: 0,
            width: frame_width as u16,
            height: frame_height as u16,
            indices: indices.to_vec(),
        });
    };

    let mut min_x = full_width;
    let mut min_y = full_height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;

    for y in 0..full_height {
        for x in 0..full_width {
            let idx = y * full_width + x;
            if indices[idx] != transparent {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found {
        return Ok(CroppedFrame {
            left: 0,
            top: 0,
            width: 1,
            height: 1,
            indices: vec![transparent],
        });
    }

    let cropped_width = max_x - min_x + 1;
    let cropped_height = max_y - min_y + 1;

    if cropped_width > u16::MAX as usize || cropped_height > u16::MAX as usize {
        bail!(
            "cropped frame exceeds GIF bounds: {}x{}",
            cropped_width,
            cropped_height
        );
    }

    let mut cropped_indices = Vec::with_capacity(cropped_width * cropped_height);
    for y in min_y..=max_y {
        let row_start = y * full_width + min_x;
        let row_end = row_start + cropped_width;
        cropped_indices.extend_from_slice(&indices[row_start..row_end]);
    }

    Ok(CroppedFrame {
        left: min_x as u16,
        top: min_y as u16,
        width: cropped_width as u16,
        height: cropped_height as u16,
        indices: cropped_indices,
    })
}

fn transparency_tolerance_from_quality(quality: u8) -> u8 {
    let q = quality.clamp(1, 100) as u32;
    (((100 - q) * 12) / 99) as u8
}

fn pixels_similar(previous: &[u8], current: &[u8], tolerance: u8) -> bool {
    if tolerance == 0 {
        return previous == current;
    }

    let tol = tolerance as i16;
    (previous[0] as i16 - current[0] as i16).abs() <= tol
        && (previous[1] as i16 - current[1] as i16).abs() <= tol
        && (previous[2] as i16 - current[2] as i16).abs() <= tol
        && (previous[3] as i16 - current[3] as i16).abs() <= tol
}

fn least_used_palette_index(indices: &[u8], palette_len: usize) -> usize {
    let mut counts = vec![0usize; palette_len];
    for &index in indices {
        let idx = index as usize;
        if idx < palette_len {
            counts[idx] += 1;
        }
    }

    counts
        .iter()
        .enumerate()
        .min_by_key(|(_, count)| *count)
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn nearest_palette_neighbor(palette: &Palette, index: usize) -> Option<usize> {
    if palette.len() <= 1 || index >= palette.len() {
        return None;
    }

    let target = palette[index];
    let mut best_idx = None;
    let mut best_distance = u32::MAX;

    for (idx, &candidate) in palette.iter().enumerate() {
        if idx == index {
            continue;
        }

        let distance = rgb_distance_sq(target, candidate);
        if distance < best_distance {
            best_distance = distance;
            best_idx = Some(idx);
        }
    }

    best_idx
}

fn flatten_palette_for_gif(palette: &Palette) -> Vec<u8> {
    let mut colors = if palette.is_empty() {
        vec![[0, 0, 0]]
    } else {
        palette.clone()
    };

    if colors.len() > 256 {
        colors.truncate(256);
    }

    let target_len = colors.len().next_power_of_two().clamp(2, 256);
    while colors.len() < target_len {
        let fallback = *colors.last().unwrap_or(&[0, 0, 0]);
        colors.push(fallback);
    }

    let mut bytes = Vec::with_capacity(colors.len() * 3);
    for color in colors {
        bytes.extend_from_slice(&color);
    }

    bytes
}

fn rgb_distance_sq(a: [u8; 3], b: [u8; 3]) -> u32 {
    let dr = a[0] as i32 - b[0] as i32;
    let dg = a[1] as i32 - b[1] as i32;
    let db = a[2] as i32 - b[2] as i32;
    (dr * dr + dg * dg + db * db) as u32
}

#[cfg(test)]
mod tests {
    use super::{crop_to_changed_region, flatten_palette_for_gif, pixels_similar};

    #[test]
    fn palette_is_padded_to_power_of_two() {
        let palette = vec![[0, 0, 0], [255, 255, 255], [127, 127, 127]];
        let flattened = flatten_palette_for_gif(&palette);
        assert_eq!(flattened.len() % 3, 0);

        let color_count = flattened.len() / 3;
        assert!(color_count.is_power_of_two());
        assert!(color_count >= 2);
        assert!(color_count <= 256);
    }

    #[test]
    fn crop_to_changed_region_shrinks_frame() {
        let transparent = 7u8;
        let mut indices = vec![transparent; 4 * 3];
        indices[5] = 1;
        indices[2 * 4 + 2] = 2;

        let cropped = crop_to_changed_region(4, 3, &indices, Some(transparent)).expect("crop succeeds");

        assert_eq!(cropped.left, 1);
        assert_eq!(cropped.top, 1);
        assert_eq!(cropped.width, 2);
        assert_eq!(cropped.height, 2);
        assert_eq!(cropped.indices, vec![1, transparent, transparent, 2]);
    }

    #[test]
    fn crop_to_changed_region_handles_fully_transparent_frames() {
        let transparent = 3u8;
        let indices = vec![transparent; 16];

        let cropped = crop_to_changed_region(4, 4, &indices, Some(transparent)).expect("crop succeeds");

        assert_eq!(cropped.left, 0);
        assert_eq!(cropped.top, 0);
        assert_eq!(cropped.width, 1);
        assert_eq!(cropped.height, 1);
        assert_eq!(cropped.indices, vec![transparent]);
    }

    #[test]
    fn pixel_similarity_uses_tolerance() {
        let prev = [100u8, 150u8, 200u8, 255u8];
        let curr = [103u8, 149u8, 198u8, 252u8];

        assert!(!pixels_similar(&prev, &curr, 0));
        assert!(pixels_similar(&prev, &curr, 3));
    }
}
