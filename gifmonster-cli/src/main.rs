use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use gifmonster_core::{encode_gif, DitherMethod, EncodeConfig, ProgressReporter};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliDitherMethod {
    #[value(name = "floyd-steinberg")]
    FloydSteinberg,
    #[value(name = "bayer")]
    Bayer,
    #[value(name = "none")]
    None,
}

impl From<CliDitherMethod> for DitherMethod {
    fn from(value: CliDitherMethod) -> Self {
        match value {
            CliDitherMethod::FloydSteinberg => DitherMethod::FloydSteinberg,
            CliDitherMethod::Bayer => DitherMethod::Bayer,
            CliDitherMethod::None => DitherMethod::None,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "gifmonster", version, about = "High-quality GIF encoder")]
struct Args {
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output: PathBuf,

    #[arg(long = "fps", value_name = "N", default_value_t = 10)]
    fps: u32,

    #[arg(long = "width", value_name = "PX")]
    width: Option<u32>,

    #[arg(long = "height", value_name = "PX")]
    height: Option<u32>,

    #[arg(
        long = "quality",
        value_name = "1-100",
        default_value_t = 90,
        value_parser = clap::value_parser!(u8).range(1..=100)
    )]
    quality: u8,

    #[arg(
        long = "dither",
        value_name = "METHOD",
        value_enum,
        default_value_t = CliDitherMethod::FloydSteinberg
    )]
    dither: CliDitherMethod,

    #[arg(long = "temporal-window", value_name = "N", default_value_t = 3)]
    temporal_window: usize,

    #[arg(long = "no-transparency")]
    no_transparency: bool,

    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

struct IndicatifReporter {
    bar: ProgressBar,
    verbose: bool,
}

impl IndicatifReporter {
    fn new(verbose: bool) -> Self {
        let bar = ProgressBar::new(0);
        let style = ProgressStyle::with_template(
            "{spinner:.green} {msg:20} [{bar:40.cyan/blue}] {pos:>4}/{len:<4}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> ");

        bar.set_style(style);
        Self { bar, verbose }
    }
}

impl ProgressReporter for IndicatifReporter {
    fn set_stage(&self, stage: &str) {
        self.bar.set_message(stage.to_owned());
        if self.verbose {
            self.bar.tick();
        }
    }

    fn set_length(&self, length: u64) {
        self.bar.set_length(length);
        self.bar.set_position(0);
    }

    fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }

    fn finish(&self, message: &str) {
        self.bar.finish_with_message(message.to_owned());
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.input.exists() {
        bail!("input path does not exist: {}", args.input.display());
    }

    let config = EncodeConfig {
        fps: args.fps,
        width: args.width,
        height: args.height,
        quality: args.quality,
        dither: args.dither.into(),
        temporal_window: args.temporal_window,
        transparency_optimization: !args.no_transparency,
        verbose: args.verbose,
    };

    let reporter = IndicatifReporter::new(args.verbose);
    let stats = encode_gif(&config, &args.input, &args.output, Some(&reporter)).with_context(|| {
        format!(
            "failed to encode input {} into output {}",
            args.input.display(),
            args.output.display()
        )
    })?;

    eprintln!("output: {}", args.output.display());
    eprintln!("frames: {}", stats.frame_count);
    eprintln!("duration: {:.2}s", stats.duration_ms as f64 / 1000.0);
    eprintln!("dimensions: {}x{}", stats.width, stats.height);
    eprintln!("file size: {} bytes", stats.file_size_bytes);

    Ok(())
}
