use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DitherMethod {
    #[default]
    FloydSteinberg,
    Bayer,
    None,
}

#[derive(Debug, Clone)]
pub struct EncodeConfig {
    pub fps: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: u8,
    pub dither: DitherMethod,
    pub temporal_window: usize,
    pub transparency_optimization: bool,
    pub verbose: bool,
}

impl Default for EncodeConfig {
    fn default() -> Self {
        Self {
            fps: 10,
            width: None,
            height: None,
            quality: 90,
            dither: DitherMethod::FloydSteinberg,
            temporal_window: 3,
            transparency_optimization: true,
            verbose: false,
        }
    }
}

impl EncodeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.fps == 0 {
            bail!("fps must be greater than 0");
        }

        if self.quality == 0 || self.quality > 100 {
            bail!("quality must be in the range 1..=100");
        }

        if self.temporal_window == 0 {
            bail!("temporal_window must be greater than 0");
        }

        if self.width == Some(0) {
            bail!("width must be greater than 0 when provided");
        }

        if self.height == Some(0) {
            bail!("height must be greater than 0 when provided");
        }

        Ok(())
    }

    pub fn frame_delay_cs(&self) -> u16 {
        ((100.0 / self.fps as f32).round() as u16).max(1)
    }
}
