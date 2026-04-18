use crate::config::EncodeConfig;
use crate::types::RgbaFrame;
use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use rayon::prelude::*;
use serde::Deserialize;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    width: u32,
    height: u32,
}

pub fn extract_frames(input: &Path, config: &EncodeConfig) -> Result<Vec<RgbaFrame>> {
    if input.is_dir() {
        load_image_frames(input, config)
    } else if input.is_file() {
        load_video_frames(input, config)
    } else {
        bail!("input path does not exist: {}", input.display());
    }
}

pub fn load_image_frames(dir: &Path, config: &EncodeConfig) -> Result<Vec<RgbaFrame>> {
    let files = list_image_files(dir)?;
    if files.is_empty() {
        bail!("no PNG/JPEG files found in {}", dir.display());
    }

    let frames: Result<Vec<RgbaFrame>> = files
        .par_iter()
        .map(|path| {
            let decoded = image::open(path)
                .with_context(|| format!("failed to decode image {}", path.display()))?
                .into_rgba8();
            let resized = resize_if_needed(decoded, config.width, config.height);
            Ok(RgbaFrame::from_rgba_image(resized))
        })
        .collect();

    frames
}

pub fn load_video_frames(input: &Path, config: &EncodeConfig) -> Result<Vec<RgbaFrame>> {
    let (src_width, src_height) = probe_video_dimensions(input)?;
    let (width, height) = compute_resize_dimensions(src_width, src_height, config.width, config.height);

    let vf = format!("fps={},scale={}:{}:flags=lanczos", config.fps, width, height);

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg("-nostdin")
        .arg("-i")
        .arg(input)
        .arg("-vf")
        .arg(vf)
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            bail!("ffmpeg not found. Install ffmpeg to encode from video files.")
        }
        Err(err) => {
            return Err(err).with_context(|| "failed to start ffmpeg subprocess");
        }
    };

    let mut stdout = child
        .stdout
        .take()
        .context("failed to capture ffmpeg stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .context("failed to capture ffmpeg stderr")?;

    let stderr_handle = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });

    let frame_bytes = width as usize * height as usize * 4;
    let mut frame_buf = vec![0u8; frame_bytes];
    let mut frames = Vec::new();

    loop {
        if !read_exact_frame_or_eof(&mut stdout, &mut frame_buf)? {
            break;
        }

        frames.push(RgbaFrame {
            width,
            height,
            pixels: frame_buf.clone(),
        });
    }

    let status = child.wait().context("failed while waiting for ffmpeg")?;
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    if !status.success() {
        let stderr_msg = String::from_utf8_lossy(&stderr_bytes);
        bail!(
            "ffmpeg failed for {}: {}",
            input.display(),
            stderr_msg.trim()
        );
    }

    if frames.is_empty() {
        bail!("no frames extracted from video {}", input.display());
    }

    Ok(frames)
}

fn probe_video_dimensions(input: &Path) -> Result<(u32, u32)> {
    let output = match Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("json")
        .arg(input)
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            bail!("ffprobe not found. Install ffmpeg to probe video metadata.")
        }
        Err(err) => return Err(err).with_context(|| "failed to run ffprobe"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "ffprobe failed for {}: {}",
            input.display(),
            stderr.trim()
        );
    }

    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("failed to parse ffprobe output for {}", input.display()))?;

    let stream = parsed
        .streams
        .first()
        .context("no video stream found in input")?;

    Ok((stream.width, stream.height))
}

fn list_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_supported_image_file(&path) {
            files.push(path);
        }
    }

    files.sort_by(|a, b| {
        let a_name = a
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let b_name = b
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        a_name.cmp(&b_name)
    });

    Ok(files)
}

fn is_supported_image_file(path: &Path) -> bool {
    let Some(ext) = path.extension() else {
        return false;
    };

    matches!(
        ext.to_string_lossy().to_lowercase().as_str(),
        "png" | "jpg" | "jpeg"
    )
}

fn resize_if_needed(
    image: image::RgbaImage,
    target_width: Option<u32>,
    target_height: Option<u32>,
) -> image::RgbaImage {
    let (src_width, src_height) = image.dimensions();
    let (dst_width, dst_height) =
        compute_resize_dimensions(src_width, src_height, target_width, target_height);

    if src_width == dst_width && src_height == dst_height {
        return image;
    }

    image::imageops::resize(&image, dst_width, dst_height, FilterType::Lanczos3)
}

fn compute_resize_dimensions(
    src_width: u32,
    src_height: u32,
    target_width: Option<u32>,
    target_height: Option<u32>,
) -> (u32, u32) {
    if target_width.is_none() && target_height.is_none() {
        return (src_width, src_height);
    }

    let scale_w = target_width
        .map(|width| width as f64 / src_width as f64)
        .unwrap_or(f64::INFINITY);
    let scale_h = target_height
        .map(|height| height as f64 / src_height as f64)
        .unwrap_or(f64::INFINITY);

    let scale = scale_w.min(scale_h).min(1.0);

    let dst_width = ((src_width as f64 * scale).round() as u32).max(1);
    let dst_height = ((src_height as f64 * scale).round() as u32).max(1);

    (dst_width, dst_height)
}

fn read_exact_frame_or_eof(reader: &mut impl Read, frame_buf: &mut [u8]) -> Result<bool> {
    let mut bytes_read = 0usize;

    while bytes_read < frame_buf.len() {
        let read = reader
            .read(&mut frame_buf[bytes_read..])
            .context("failed while reading ffmpeg frame stream")?;

        if read == 0 {
            if bytes_read == 0 {
                return Ok(false);
            }

            bail!(
                "unexpected EOF while reading ffmpeg frame (read {}/{})",
                bytes_read,
                frame_buf.len()
            );
        }

        bytes_read += read;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::compute_resize_dimensions;

    #[test]
    fn keeps_aspect_ratio_when_both_limits_are_set() {
        let (w, h) = compute_resize_dimensions(1920, 1080, Some(800), Some(800));
        assert_eq!((w, h), (800, 450));
    }

    #[test]
    fn does_not_upscale_if_target_is_larger() {
        let (w, h) = compute_resize_dimensions(320, 180, Some(1920), Some(1080));
        assert_eq!((w, h), (320, 180));
    }
}
