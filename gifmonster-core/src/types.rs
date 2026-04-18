use anyhow::{anyhow, bail, Result};
use image::RgbaImage;

pub type Palette = Vec<[u8; 3]>;

#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaFrame {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            bail!(
                "invalid RGBA frame buffer length: expected {}, got {}",
                expected,
                pixels.len()
            );
        }

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn from_rgba_image(image: RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        Self {
            width,
            height,
            pixels: image.into_raw(),
        }
    }

    pub fn to_rgba_image(&self) -> Result<RgbaImage> {
        RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .ok_or_else(|| anyhow!("failed to build image buffer from RGBA frame data"))
    }

    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

#[derive(Debug, Clone)]
pub struct IndexedFrame {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
    pub palette: Palette,
    pub delay_cs: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct EncodeStats {
    pub file_size_bytes: u64,
    pub frame_count: usize,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
}

pub trait ProgressReporter: Send + Sync {
    fn set_stage(&self, _stage: &str) {}
    fn set_length(&self, _length: u64) {}
    fn inc(&self, _delta: u64) {}
    fn finish(&self, _message: &str) {}
}
