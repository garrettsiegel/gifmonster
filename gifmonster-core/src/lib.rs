pub mod config;
pub mod dither;
pub mod encode;
pub mod extract;
pub mod quantize;
pub mod types;

pub use config::{DitherMethod, EncodeConfig};
pub use types::{EncodeStats, IndexedFrame, Palette, ProgressReporter, RgbaFrame};

use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use std::path::Path;

pub fn encode_gif(
    config: &EncodeConfig,
    input: &Path,
    output: &Path,
    progress: Option<&dyn ProgressReporter>,
) -> Result<EncodeStats> {
    config.validate()?;

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output directory {}", parent.display()))?;
        }
    }

    if let Some(reporter) = progress {
        reporter.set_stage("Extracting frames");
    }

    let frames = extract::extract_frames(input, config)
        .with_context(|| format!("failed to extract frames from {}", input.display()))?;

    if frames.is_empty() {
        bail!("no frames found in input");
    }

    ensure_consistent_dimensions(&frames)?;

    if let Some(reporter) = progress {
        reporter.set_stage("Quantizing frames");
        reporter.set_length((frames.len() * 2) as u64);
    }

    let mut palettes: Vec<Palette> = frames
        .par_iter()
        .map(|frame| quantize::median_cut_with_quality(frame, 256, config.quality))
        .collect();

    quantize::smooth_palettes(&mut palettes, config.temporal_window, config.quality);

    let delay_cs = config.frame_delay_cs();
    let indexed_frames: Vec<IndexedFrame> = frames
        .par_iter()
        .zip(palettes.par_iter())
        .map(|(frame, palette)| IndexedFrame {
            width: frame.width,
            height: frame.height,
            indices: dither::apply_dither(frame, palette, config.dither, config.quality),
            palette: palette.clone(),
            delay_cs,
        })
        .collect();

    if let Some(reporter) = progress {
        reporter.inc(frames.len() as u64);
        reporter.set_stage("Encoding GIF");
    }

    let stats = encode::encode_indexed_frames(output, &indexed_frames, &frames, config, progress)
        .with_context(|| format!("failed to write GIF to {}", output.display()))?;

    if let Some(reporter) = progress {
        reporter.finish("Done");
    }

    Ok(stats)
}

fn ensure_consistent_dimensions(frames: &[RgbaFrame]) -> Result<()> {
    let first = &frames[0];
    for (idx, frame) in frames.iter().enumerate().skip(1) {
        if frame.width != first.width || frame.height != first.height {
            bail!(
                "frame {} has mismatched dimensions {}x{} (expected {}x{})",
                idx,
                frame.width,
                frame.height,
                first.width,
                first.height
            );
        }
    }

    Ok(())
}
