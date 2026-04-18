use crate::config::DitherMethod;
use crate::quantize::{map_to_palette, nearest_palette_index_with_labs, palette_to_oklab};
use crate::types::{Palette, RgbaFrame};

const BAYER_8X8: [[u8; 8]; 8] = [
    [0, 48, 12, 60, 3, 51, 15, 63],
    [32, 16, 44, 28, 35, 19, 47, 31],
    [8, 56, 4, 52, 11, 59, 7, 55],
    [40, 24, 36, 20, 43, 27, 39, 23],
    [2, 50, 14, 62, 1, 49, 13, 61],
    [34, 18, 46, 30, 33, 17, 45, 29],
    [10, 58, 6, 54, 9, 57, 5, 53],
    [42, 26, 38, 22, 41, 25, 37, 21],
];

pub fn apply_dither(
    frame: &RgbaFrame,
    palette: &Palette,
    method: DitherMethod,
    quality: u8,
) -> Vec<u8> {
    let strength = dither_strength_from_quality(quality);

    match method {
        DitherMethod::FloydSteinberg => floyd_steinberg_with_strength(frame, palette, strength),
        DitherMethod::Bayer => bayer_dither_with_strength(frame, palette, strength),
        DitherMethod::None => map_to_palette(frame, palette),
    }
}

pub fn floyd_steinberg(frame: &RgbaFrame, palette: &Palette) -> Vec<u8> {
    floyd_steinberg_with_strength(frame, palette, 1.0)
}

fn floyd_steinberg_with_strength(frame: &RgbaFrame, palette: &Palette, strength: f32) -> Vec<u8> {
    let pixel_count = frame.pixel_count();
    if pixel_count == 0 || palette.is_empty() {
        return vec![0u8; pixel_count];
    }

    let width = frame.width as usize;
    let height = frame.height as usize;

    let mut r_channel = Vec::with_capacity(pixel_count);
    let mut g_channel = Vec::with_capacity(pixel_count);
    let mut b_channel = Vec::with_capacity(pixel_count);

    for px in frame.pixels.chunks_exact(4) {
        r_channel.push(px[0] as f32);
        g_channel.push(px[1] as f32);
        b_channel.push(px[2] as f32);
    }

    let mut indices = vec![0u8; pixel_count];
    let palette_labs = palette_to_oklab(palette);
    let strength = strength.clamp(0.0, 1.0);

    for y in 0..height {
        let left_to_right = y % 2 == 0;
        if left_to_right {
            for x in 0..width {
                let idx = y * width + x;

                let r = r_channel[idx].clamp(0.0, 255.0);
                let g = g_channel[idx].clamp(0.0, 255.0);
                let b = b_channel[idx].clamp(0.0, 255.0);

                let palette_idx =
                    nearest_palette_index_with_labs(&palette_labs, r as u8, g as u8, b as u8);
                indices[idx] = palette_idx;

                let chosen = palette[palette_idx as usize];

                let err_r = (r - chosen[0] as f32) * strength;
                let err_g = (g - chosen[1] as f32) * strength;
                let err_b = (b - chosen[2] as f32) * strength;

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize,
                    err_r * 7.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize,
                    err_g * 7.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize,
                    err_b * 7.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_r * 3.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_g * 3.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_b * 3.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_r * 5.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_g * 5.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_b * 5.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_r * 1.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_g * 1.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_b * 1.0 / 16.0,
                );
            }
        } else {
            for x in (0..width).rev() {
                let idx = y * width + x;

                let r = r_channel[idx].clamp(0.0, 255.0);
                let g = g_channel[idx].clamp(0.0, 255.0);
                let b = b_channel[idx].clamp(0.0, 255.0);

                let palette_idx =
                    nearest_palette_index_with_labs(&palette_labs, r as u8, g as u8, b as u8);
                indices[idx] = palette_idx;

                let chosen = palette[palette_idx as usize];

                let err_r = (r - chosen[0] as f32) * strength;
                let err_g = (g - chosen[1] as f32) * strength;
                let err_b = (b - chosen[2] as f32) * strength;

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize,
                    err_r * 7.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize,
                    err_g * 7.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize,
                    err_b * 7.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_r * 3.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_g * 3.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize + 1,
                    y as isize + 1,
                    err_b * 3.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_r * 5.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_g * 5.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize,
                    y as isize + 1,
                    err_b * 5.0 / 16.0,
                );

                add_error(
                    &mut r_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_r * 1.0 / 16.0,
                );
                add_error(
                    &mut g_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_g * 1.0 / 16.0,
                );
                add_error(
                    &mut b_channel,
                    width,
                    height,
                    x as isize - 1,
                    y as isize + 1,
                    err_b * 1.0 / 16.0,
                );
            }
        }
    }

    indices
}

pub fn bayer_dither(frame: &RgbaFrame, palette: &Palette) -> Vec<u8> {
    bayer_dither_with_strength(frame, palette, 1.0)
}

fn bayer_dither_with_strength(frame: &RgbaFrame, palette: &Palette, strength: f32) -> Vec<u8> {
    let pixel_count = frame.pixel_count();
    if pixel_count == 0 || palette.is_empty() {
        return vec![0u8; pixel_count];
    }

    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut indices = Vec::with_capacity(pixel_count);
    let palette_labs = palette_to_oklab(palette);
    let strength = strength.clamp(0.0, 1.0);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let px = &frame.pixels[idx..idx + 4];

            let threshold = BAYER_8X8[y % 8][x % 8] as f32 / 63.0 - 0.5;
            let offset = threshold * 32.0 * strength;

            let r = (px[0] as f32 + offset).clamp(0.0, 255.0) as u8;
            let g = (px[1] as f32 + offset).clamp(0.0, 255.0) as u8;
            let b = (px[2] as f32 + offset).clamp(0.0, 255.0) as u8;

            indices.push(nearest_palette_index_with_labs(&palette_labs, r, g, b));
        }
    }

    indices
}

fn dither_strength_from_quality(quality: u8) -> f32 {
    let q = quality.clamp(1, 100) as f32 / 100.0;
    // Higher quality keeps stronger dithering; lower quality trades some noise for better compression.
    0.55 + q * 0.45
}

fn add_error(
    channel: &mut [f32],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
    amount: f32,
) {
    if x < 0 || y < 0 {
        return;
    }

    let x = x as usize;
    let y = y as usize;

    if x >= width || y >= height {
        return;
    }

    let idx = y * width + x;
    channel[idx] += amount;
}

#[cfg(test)]
mod tests {
    use super::{bayer_dither, floyd_steinberg};
    use crate::types::{Palette, RgbaFrame};

    fn gradient_frame(width: u32, height: u32) -> RgbaFrame {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let v = ((x + y) * 255 / (width + height - 2).max(1)) as u8;
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
        }

        RgbaFrame::new(width, height, pixels).expect("valid frame")
    }

    fn grayscale_palette() -> Palette {
        (0..=255)
            .step_by(16)
            .map(|v| [v as u8, v as u8, v as u8])
            .collect()
    }

    #[test]
    fn floyd_steinberg_indices_are_valid() {
        let frame = gradient_frame(32, 32);
        let palette = grayscale_palette();
        let indices = floyd_steinberg(&frame, &palette);

        assert_eq!(indices.len(), frame.pixel_count());
        assert!(indices.iter().all(|idx| (*idx as usize) < palette.len()));
    }

    #[test]
    fn bayer_indices_are_valid() {
        let frame = gradient_frame(32, 32);
        let palette = grayscale_palette();
        let indices = bayer_dither(&frame, &palette);

        assert_eq!(indices.len(), frame.pixel_count());
        assert!(indices.iter().all(|idx| (*idx as usize) < palette.len()));
    }
}
