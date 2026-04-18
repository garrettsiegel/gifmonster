use crate::types::{Palette, RgbaFrame};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
struct ColorCount {
    color: [u8; 3],
    lab: [f32; 3],
    count: u32,
}

#[derive(Clone, Copy, Debug)]
struct AnchorColor {
    lab: [f32; 3],
    weight: f32,
}

#[derive(Clone, Debug)]
struct AnchorCluster {
    sum: [f64; 3],
    weight: f64,
}

impl AnchorCluster {
    fn new(lab: [f32; 3], weight: f32) -> Self {
        Self {
            sum: [
                lab[0] as f64 * weight as f64,
                lab[1] as f64 * weight as f64,
                lab[2] as f64 * weight as f64,
            ],
            weight: weight as f64,
        }
    }

    fn centroid(&self) -> [f32; 3] {
        if self.weight <= 0.0 {
            return [0.0, 0.0, 0.0];
        }

        [
            (self.sum[0] / self.weight) as f32,
            (self.sum[1] / self.weight) as f32,
            (self.sum[2] / self.weight) as f32,
        ]
    }

    fn add_sample(&mut self, lab: [f32; 3], weight: f32) {
        self.sum[0] += lab[0] as f64 * weight as f64;
        self.sum[1] += lab[1] as f64 * weight as f64;
        self.sum[2] += lab[2] as f64 * weight as f64;
        self.weight += weight as f64;
    }
}

#[derive(Clone, Debug)]
struct Bucket {
    colors: Vec<ColorCount>,
    total_count: u64,
    min: [f32; 3],
    max: [f32; 3],
}

impl Bucket {
    fn new(colors: Vec<ColorCount>) -> Self {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut total_count = 0u64;

        for entry in &colors {
            total_count += entry.count as u64;
            for channel in 0..3 {
                min[channel] = min[channel].min(entry.lab[channel]);
                max[channel] = max[channel].max(entry.lab[channel]);
            }
        }

        Self {
            colors,
            total_count,
            min,
            max,
        }
    }

    fn is_splittable(&self) -> bool {
        self.colors.len() > 1
    }

    fn dominant_channel(&self) -> usize {
        let mut best_channel = 0usize;
        let mut best_range = 0.0f32;

        for channel in 0..3 {
            let range = self.max[channel] - self.min[channel];
            if range > best_range {
                best_range = range;
                best_channel = channel;
            }
        }

        best_channel
    }

    fn score(&self) -> f32 {
        let max_range = (0..3)
            .map(|channel| self.max[channel] - self.min[channel])
            .fold(0.0f32, f32::max);

        max_range * self.total_count as f32
    }

    fn split_owned(mut self) -> Option<(Bucket, Bucket)> {
        if !self.is_splittable() {
            return None;
        }

        let channel = self.dominant_channel();
        self.colors
            .sort_by(|a, b| a.lab[channel].total_cmp(&b.lab[channel]));

        let half = self.total_count / 2;
        let mut cumulative = 0u64;
        let mut split_at = self.colors.len() / 2;

        for (idx, entry) in self.colors.iter().enumerate() {
            cumulative += entry.count as u64;
            if cumulative >= half {
                split_at = (idx + 1).min(self.colors.len() - 1);
                break;
            }
        }

        let right = self.colors.split_off(split_at);
        let left = self.colors;

        if left.is_empty() || right.is_empty() {
            return None;
        }

        Some((Bucket::new(left), Bucket::new(right)))
    }

    fn average_color(&self) -> [u8; 3] {
        if self.total_count == 0 {
            return [0, 0, 0];
        }

        let mut r_sum = 0u64;
        let mut g_sum = 0u64;
        let mut b_sum = 0u64;

        for entry in &self.colors {
            let weight = entry.count as u64;
            r_sum += entry.color[0] as u64 * weight;
            g_sum += entry.color[1] as u64 * weight;
            b_sum += entry.color[2] as u64 * weight;
        }

        [
            (r_sum / self.total_count) as u8,
            (g_sum / self.total_count) as u8,
            (b_sum / self.total_count) as u8,
        ]
    }
}

pub fn median_cut(frame: &RgbaFrame, max_colors: usize) -> Palette {
    median_cut_with_quality(frame, max_colors, 100)
}

pub fn median_cut_with_quality(frame: &RgbaFrame, max_colors: usize, quality: u8) -> Palette {
    if frame.pixels.is_empty() {
        return vec![[0, 0, 0]];
    }

    let max_colors = max_colors.clamp(1, 256);
    let mut histogram = HashMap::<u32, u32>::with_capacity(frame.pixel_count().min(1 << 16));
    let color_shift = quality_color_shift(quality);

    for px in frame.pixels.chunks_exact(4) {
        let r = quantize_channel(px[0], color_shift);
        let g = quantize_channel(px[1], color_shift);
        let b = quantize_channel(px[2], color_shift);
        let key = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        *histogram.entry(key).or_insert(0) += 1;
    }

    if histogram.len() <= max_colors {
        let mut ordered: Vec<(u32, u32)> = histogram.into_iter().collect();
        ordered.sort_by_key(|entry| Reverse(entry.1));

        return ordered
            .into_iter()
            .map(|(rgb, _)| unpack_rgb(rgb))
            .collect();
    }

    let initial_colors: Vec<ColorCount> = histogram
        .into_iter()
        .map(|(rgb, count)| {
            let color = unpack_rgb(rgb);
            ColorCount {
                color,
                lab: rgb_to_oklab(color),
                count,
            }
        })
        .collect();
    let samples = initial_colors.clone();

    let mut buckets = vec![Bucket::new(initial_colors)];

    while buckets.len() < max_colors {
        let Some((idx, _)) = buckets
            .iter()
            .enumerate()
            .filter(|(_, bucket)| bucket.is_splittable())
            .max_by(|(_, a), (_, b)| a.score().total_cmp(&b.score()))
        else {
            break;
        };

        let Some((left, right)) = buckets.swap_remove(idx).split_owned() else {
            break;
        };

        buckets.push(left);
        buckets.push(right);
    }

    let mut palette: Palette = buckets.iter().map(Bucket::average_color).collect();
    if palette.is_empty() {
        palette.push([0, 0, 0]);
    }

    if palette.len() > max_colors {
        palette.truncate(max_colors);
    }

    refine_palette_with_kmeans(&mut palette, &samples, kmeans_iterations_for_quality(quality));

    palette
}

pub fn map_to_palette(frame: &RgbaFrame, palette: &Palette) -> Vec<u8> {
    if palette.is_empty() {
        return vec![0; frame.pixel_count()];
    }

    let mut cache = HashMap::<u32, u8>::with_capacity(frame.pixel_count().min(1 << 16));
    let palette_labs = palette_to_oklab(palette);

    frame
        .pixels
        .chunks_exact(4)
        .map(|px| {
            let key = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | (px[2] as u32);
            if let Some(idx) = cache.get(&key) {
                return *idx;
            }

            let idx = nearest_palette_index_with_labs(&palette_labs, px[0], px[1], px[2]);
            cache.insert(key, idx);
            idx
        })
        .collect()
}

pub fn smooth_palettes(palettes: &mut [Palette], window: usize, quality: u8) {
    if palettes.len() <= 1 {
        return;
    }

    if window > 1 {
        let snapshot = palettes.to_vec();
        let snapshot_labs: Vec<Vec<[f32; 3]>> = snapshot.iter().map(palette_to_oklab).collect();
        let radius = window / 2;
        let threshold = quality_merge_threshold(quality);
        let threshold_sq = threshold * threshold;

        for frame_idx in 0..palettes.len() {
            let start = frame_idx.saturating_sub(radius);
            let end = (frame_idx + radius + 1).min(snapshot.len());

            let mut smoothed = snapshot[frame_idx].clone();

            for color in &mut smoothed {
                let base_lab = rgb_to_oklab(*color);
                let mut l_acc = base_lab[0] * 2.0;
                let mut a_acc = base_lab[1] * 2.0;
                let mut b_acc = base_lab[2] * 2.0;
                let mut total_weight = 2.0f32;

                for (offset, neighbor_labs) in snapshot_labs[start..end].iter().enumerate() {
                    let neighbor_idx = start + offset;
                    if neighbor_idx == frame_idx {
                        continue;
                    }

                    let frame_distance = neighbor_idx.abs_diff(frame_idx) as f32;
                    let neighbor_weight = 1.0f32 / (1.0 + frame_distance);

                    let Some((neighbor_lab, dist_sq)) =
                        nearest_lab_and_distance(neighbor_labs, base_lab)
                    else {
                        continue;
                    };

                    if dist_sq > threshold_sq {
                        continue;
                    }

                    l_acc += neighbor_lab[0] * neighbor_weight;
                    a_acc += neighbor_lab[1] * neighbor_weight;
                    b_acc += neighbor_lab[2] * neighbor_weight;
                    total_weight += neighbor_weight;
                }

                *color = oklab_to_srgb([
                    l_acc / total_weight,
                    a_acc / total_weight,
                    b_acc / total_weight,
                ]);
            }

            merge_similar_colors(&mut smoothed, threshold / 2.0);
            palettes[frame_idx] = smoothed;
        }
    }

    apply_global_anchor_stabilization(palettes, quality);
}

pub fn quality_merge_threshold(quality: u8) -> f32 {
    let quality = quality.clamp(1, 100) as f32;
    ((100.0 - quality) / 100.0 * 0.12) + 0.01
}

pub(crate) fn nearest_palette_index_with_labs(
    palette_labs: &[[f32; 3]],
    r: u8,
    g: u8,
    b: u8,
) -> u8 {
    if palette_labs.is_empty() {
        return 0;
    }

    let target_lab = rgb_to_oklab([r, g, b]);

    let mut best_index = 0usize;
    let mut best_distance = f32::INFINITY;

    for (idx, candidate_lab) in palette_labs.iter().enumerate() {
        let distance = oklab_distance_sq(*candidate_lab, target_lab);
        if distance < best_distance {
            best_distance = distance;
            best_index = idx;
        }
    }

    best_index as u8
}

pub(crate) fn palette_to_oklab(palette: &Palette) -> Vec<[f32; 3]> {
    palette.iter().copied().map(rgb_to_oklab).collect()
}

fn nearest_lab_and_distance(palette_labs: &[[f32; 3]], target: [f32; 3]) -> Option<([f32; 3], f32)> {
    let first = *palette_labs.first()?;

    let mut best = first;
    let mut best_distance = oklab_distance_sq(first, target);

    for &candidate in palette_labs.iter().skip(1) {
        let distance = oklab_distance_sq(candidate, target);
        if distance < best_distance {
            best_distance = distance;
            best = candidate;
        }
    }

    Some((best, best_distance))
}

fn merge_similar_colors(palette: &mut Palette, threshold: f32) {
    let threshold_sq = threshold * threshold;
    let mut palette_labs = palette_to_oklab(palette);

    for idx in 0..palette.len() {
        for prev_idx in 0..idx {
            let distance = oklab_distance_sq(palette_labs[idx], palette_labs[prev_idx]);
            if distance <= threshold_sq {
                palette[idx] = palette[prev_idx];
                palette_labs[idx] = palette_labs[prev_idx];
                break;
            }
        }
    }
}

fn refine_palette_with_kmeans(palette: &mut Palette, samples: &[ColorCount], iterations: usize) {
    if palette.is_empty() || samples.is_empty() || iterations == 0 {
        return;
    }

    let mut centers = palette_to_oklab(palette);
    let cluster_count = centers.len();

    for _ in 0..iterations {
        let mut sums = vec![[0.0f64; 3]; cluster_count];
        let mut counts = vec![0.0f64; cluster_count];
        let mut max_shift = 0.0f32;

        for sample in samples {
            let idx = nearest_center_index(&centers, sample.lab);
            let weight = sample.count as f64;
            sums[idx][0] += sample.lab[0] as f64 * weight;
            sums[idx][1] += sample.lab[1] as f64 * weight;
            sums[idx][2] += sample.lab[2] as f64 * weight;
            counts[idx] += weight;
        }

        for idx in 0..cluster_count {
            if counts[idx] > 0.0 {
                let previous = centers[idx];
                centers[idx][0] = (sums[idx][0] / counts[idx]) as f32;
                centers[idx][1] = (sums[idx][1] / counts[idx]) as f32;
                centers[idx][2] = (sums[idx][2] / counts[idx]) as f32;
                max_shift = max_shift.max(oklab_distance_sq(previous, centers[idx]));
            }
        }

        if max_shift < 1e-6 {
            break;
        }
    }

    for (color, center) in palette.iter_mut().zip(centers) {
        *color = oklab_to_srgb(center);
    }
}

fn kmeans_iterations_for_quality(quality: u8) -> usize {
    match quality.clamp(1, 100) {
        95..=100 => 6,
        80..=94 => 5,
        60..=79 => 4,
        40..=59 => 3,
        _ => 2,
    }
}

fn quality_color_shift(quality: u8) -> u8 {
    match quality.clamp(1, 100) {
        95..=100 => 0,
        85..=94 => 1,
        70..=84 => 2,
        50..=69 => 3,
        _ => 4,
    }
}

fn quantize_channel(channel: u8, shift: u8) -> u8 {
    if shift == 0 {
        return channel;
    }

    let step = 1u16 << shift;
    let base = ((channel as u16) >> shift) << shift;
    let centered = base + step / 2;
    centered.min(255) as u8
}

fn apply_global_anchor_stabilization(palettes: &mut [Palette], quality: u8) {
    let anchors = derive_global_anchors(palettes, quality);
    if anchors.is_empty() {
        return;
    }

    let replace_threshold = anchor_replace_threshold(quality);
    let replace_threshold_sq = replace_threshold * replace_threshold;
    let pull = anchor_pull_strength(quality);
    let merge_threshold = quality_merge_threshold(quality) / 2.0;

    for palette in palettes.iter_mut() {
        if palette.is_empty() {
            continue;
        }

        let mut labs = palette_to_oklab(palette);

        for anchor in &anchors {
            let nearest_idx = nearest_center_index(&labs, anchor.lab);
            let nearest_dist_sq = oklab_distance_sq(labs[nearest_idx], anchor.lab);

            if nearest_dist_sq <= replace_threshold_sq {
                labs[nearest_idx] = blend_lab(labs[nearest_idx], anchor.lab, pull);
            } else {
                let replacement_idx = farthest_index_from_anchors(&labs, &anchors);
                labs[replacement_idx] = blend_lab(labs[replacement_idx], anchor.lab, pull);
            }
        }

        for (color, lab) in palette.iter_mut().zip(labs) {
            *color = oklab_to_srgb(lab);
        }

        merge_similar_colors(palette, merge_threshold);
    }
}

fn derive_global_anchors(palettes: &[Palette], quality: u8) -> Vec<AnchorColor> {
    if palettes.is_empty() {
        return Vec::new();
    }

    let cluster_threshold = anchor_cluster_threshold(quality);
    let cluster_threshold_sq = cluster_threshold * cluster_threshold;
    let mut clusters: Vec<AnchorCluster> = Vec::new();

    for palette in palettes {
        let palette_size = palette.len().max(1) as f32;
        for &color in palette {
            let lab = rgb_to_oklab(color);
            let weight = 1.0f32 / palette_size;

            let mut merged = false;
            for cluster in &mut clusters {
                let centroid = cluster.centroid();
                if oklab_distance_sq(centroid, lab) <= cluster_threshold_sq {
                    cluster.add_sample(lab, weight);
                    merged = true;
                    break;
                }
            }

            if !merged {
                clusters.push(AnchorCluster::new(lab, weight));
            }
        }
    }

    clusters.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    let anchor_count = desired_anchor_count(quality);

    clusters
        .into_iter()
        .take(anchor_count)
        .map(|cluster| AnchorColor {
            lab: cluster.centroid(),
            weight: cluster.weight as f32,
        })
        .collect()
}

fn desired_anchor_count(quality: u8) -> usize {
    let quality = quality.clamp(1, 100) as usize;
    (6 + ((100 - quality) / 8)).clamp(6, 16)
}

fn anchor_cluster_threshold(quality: u8) -> f32 {
    let quality = quality.clamp(1, 100) as f32;
    0.035 + ((100.0 - quality) / 100.0 * 0.08)
}

fn anchor_replace_threshold(quality: u8) -> f32 {
    let quality = quality.clamp(1, 100) as f32;
    0.020 + ((100.0 - quality) / 100.0 * 0.060)
}

fn anchor_pull_strength(quality: u8) -> f32 {
    let quality = quality.clamp(1, 100) as f32;
    (0.35 + ((100.0 - quality) / 100.0 * 0.30)).clamp(0.35, 0.65)
}

fn farthest_index_from_anchors(colors: &[[f32; 3]], anchors: &[AnchorColor]) -> usize {
    if colors.is_empty() {
        return 0;
    }

    colors
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            min_anchor_distance_sq(**a, anchors).total_cmp(&min_anchor_distance_sq(**b, anchors))
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn min_anchor_distance_sq(color: [f32; 3], anchors: &[AnchorColor]) -> f32 {
    anchors
        .iter()
        .map(|anchor| {
            // Higher-weight anchors are treated as slightly closer to encourage stable colors.
            oklab_distance_sq(color, anchor.lab) / (1.0 + anchor.weight)
        })
        .fold(f32::INFINITY, f32::min)
}

fn blend_lab(base: [f32; 3], target: [f32; 3], weight: f32) -> [f32; 3] {
    let t = weight.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + target[0] * t,
        base[1] * (1.0 - t) + target[1] * t,
        base[2] * (1.0 - t) + target[2] * t,
    ]
}

fn nearest_center_index(centers: &[[f32; 3]], target: [f32; 3]) -> usize {
    let mut best_idx = 0usize;
    let mut best_distance = f32::INFINITY;

    for (idx, center) in centers.iter().enumerate() {
        let distance = oklab_distance_sq(*center, target);
        if distance < best_distance {
            best_distance = distance;
            best_idx = idx;
        }
    }

    best_idx
}

fn oklab_distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dl = a[0] - b[0];
    let da = a[1] - b[1];
    let db = a[2] - b[2];
    dl * dl + da * da + db * db
}

fn rgb_to_oklab(rgb: [u8; 3]) -> [f32; 3] {
    let lut = srgb_linear_lut();
    let r = lut[rgb[0] as usize];
    let g = lut[rgb[1] as usize];
    let b = lut[rgb[2] as usize];

    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;

    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

fn oklab_to_srgb(lab: [f32; 3]) -> [u8; 3] {
    let l_ = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m_ = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
    let s_ = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let r_linear = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let g_linear = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let b_linear = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;

    [
        linear_to_srgb(r_linear),
        linear_to_srgb(g_linear),
        linear_to_srgb(b_linear),
    ]
}

fn srgb_linear_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut values = [0.0f32; 256];
        let mut idx = 0usize;
        while idx < 256 {
            values[idx] = srgb_to_linear_formula(idx as u8);
            idx += 1;
        }
        values
    })
}

fn srgb_to_linear_formula(channel: u8) -> f32 {
    let v = channel as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> u8 {
    let v = channel.clamp(0.0, 1.0);
    let srgb = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };

    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

fn unpack_rgb(value: u32) -> [u8; 3] {
    [
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        derive_global_anchors, map_to_palette, median_cut, median_cut_with_quality,
        palette_to_oklab, quality_merge_threshold, smooth_palettes,
    };
    use crate::types::{Palette, RgbaFrame};

    fn gradient_frame(width: u32, height: u32) -> RgbaFrame {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let r = (x * 255 / (width - 1).max(1)) as u8;
                let g = (y * 255 / (height - 1).max(1)) as u8;
                let b = ((x + y) % 256) as u8;
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }

        RgbaFrame::new(width, height, pixels).expect("valid frame")
    }

    #[test]
    fn median_cut_returns_at_most_requested_colors() {
        let frame = gradient_frame(64, 64);
        let palette = median_cut(&frame, 256);
        assert!(!palette.is_empty());
        assert!(palette.len() <= 256);
    }

    #[test]
    fn mapped_indices_stay_in_palette_bounds() {
        let frame = gradient_frame(32, 32);
        let palette = median_cut(&frame, 64);
        let indices = map_to_palette(&frame, &palette);

        assert_eq!(indices.len(), frame.pixel_count());
        assert!(indices.iter().all(|idx| (*idx as usize) < palette.len()));
    }

    #[test]
    fn quality_threshold_scales_as_expected() {
        let high_quality = quality_merge_threshold(100);
        let low_quality = quality_merge_threshold(10);
        assert!(low_quality > high_quality);
    }

    #[test]
    fn palette_oklab_conversion_matches_length() {
        let palette: Palette = vec![[0, 0, 0], [255, 255, 255], [255, 0, 0]];
        let labs = palette_to_oklab(&palette);
        assert_eq!(labs.len(), palette.len());
    }

    #[test]
    fn temporal_smoothing_preserves_palette_count() {
        let mut palettes: Vec<Palette> = vec![
            vec![[10, 20, 30], [90, 100, 110], [200, 210, 220]],
            vec![[12, 22, 32], [89, 99, 109], [205, 215, 225]],
            vec![[11, 21, 31], [91, 101, 111], [198, 208, 218]],
        ];

        let lengths_before: Vec<usize> = palettes.iter().map(Vec::len).collect();
        smooth_palettes(&mut palettes, 3, 70);
        let lengths_after: Vec<usize> = palettes.iter().map(Vec::len).collect();

        assert_eq!(lengths_before, lengths_after);
    }

    #[test]
    fn global_anchor_extraction_finds_stable_colors() {
        let palettes: Vec<Palette> = vec![
            vec![[220, 30, 30], [30, 30, 30], [240, 240, 240]],
            vec![[222, 32, 32], [28, 30, 31], [238, 239, 241]],
            vec![[219, 29, 31], [31, 31, 30], [241, 241, 239]],
        ];

        let anchors = derive_global_anchors(&palettes, 80);
        assert!(!anchors.is_empty());
    }

    #[test]
    fn smoothing_with_window_one_still_keeps_palette_lengths() {
        let mut palettes: Vec<Palette> = vec![
            vec![[200, 20, 20], [10, 10, 10], [230, 230, 230]],
            vec![[205, 23, 23], [13, 12, 12], [228, 229, 231]],
            vec![[198, 18, 19], [8, 9, 10], [232, 231, 228]],
        ];

        let before: Vec<usize> = palettes.iter().map(Vec::len).collect();
        smooth_palettes(&mut palettes, 1, 75);
        let after: Vec<usize> = palettes.iter().map(Vec::len).collect();

        assert_eq!(before, after);
    }

    #[test]
    fn lower_quality_reduces_unique_palette_pressure() {
        let frame = gradient_frame(128, 128);
        let high = median_cut_with_quality(&frame, 256, 100);
        let low = median_cut_with_quality(&frame, 256, 35);
        assert!(low.len() <= high.len());
    }
}
