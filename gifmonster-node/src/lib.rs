use anyhow::{bail, Context, Result};
use gifmonster_core::{DitherMethod, EncodeConfig, EncodeStats, ProgressReporter};
use napi::bindgen_prelude::{AsyncTask, Env, Function, Task};
use napi::Error;
use napi::Status;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::path::Path;

type ProgressCallback = ThreadsafeFunction<ProgressEvent, (), ProgressEvent, Status, false>;

#[napi(object)]
pub struct GifEncodeOptions {
    pub fps: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: Option<u8>,
    pub dither: Option<String>,
    pub temporal_window: Option<u32>,
    pub transparency_optimization: Option<bool>,
    pub verbose: Option<bool>,
}

#[napi(object)]
pub struct GifEncodeResult {
    pub file_size_bytes: f64,
    pub frame_count: u32,
    pub duration_ms: f64,
    pub width: u32,
    pub height: u32,
}

#[napi(object)]
pub struct ProgressEvent {
    pub event: String,
    pub stage: Option<String>,
    pub length: Option<f64>,
    pub delta: Option<f64>,
    pub message: Option<String>,
}

impl ProgressEvent {
    fn stage(value: &str) -> Self {
        Self {
            event: "stage".to_owned(),
            stage: Some(value.to_owned()),
            length: None,
            delta: None,
            message: None,
        }
    }

    fn length(value: u64) -> Self {
        Self {
            event: "length".to_owned(),
            stage: None,
            length: Some(value as f64),
            delta: None,
            message: None,
        }
    }

    fn progress(delta: u64) -> Self {
        Self {
            event: "progress".to_owned(),
            stage: None,
            length: None,
            delta: Some(delta as f64),
            message: None,
        }
    }

    fn finish(value: &str) -> Self {
        Self {
            event: "finish".to_owned(),
            stage: None,
            length: None,
            delta: None,
            message: Some(value.to_owned()),
        }
    }
}

pub struct EncodeGifTask {
    input: String,
    output: String,
    config: EncodeConfig,
    progress_callback: Option<ProgressCallback>,
}

struct NapiProgressReporter<'a> {
    callback: &'a ProgressCallback,
}

impl<'a> NapiProgressReporter<'a> {
    fn emit(&self, event: ProgressEvent) {
        let _ = self
            .callback
            .call(event, ThreadsafeFunctionCallMode::NonBlocking);
    }
}

impl ProgressReporter for NapiProgressReporter<'_> {
    fn set_stage(&self, stage: &str) {
        self.emit(ProgressEvent::stage(stage));
    }

    fn set_length(&self, length: u64) {
        self.emit(ProgressEvent::length(length));
    }

    fn inc(&self, delta: u64) {
        self.emit(ProgressEvent::progress(delta));
    }

    fn finish(&self, message: &str) {
        self.emit(ProgressEvent::finish(message));
    }
}

impl Task for EncodeGifTask {
    type Output = EncodeStats;
    type JsValue = GifEncodeResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let reporter = self
            .progress_callback
            .as_ref()
            .map(|callback| NapiProgressReporter { callback });

        let progress = reporter.as_ref().map(|r| r as &dyn ProgressReporter);

        gifmonster_core::encode_gif(
            &self.config,
            Path::new(&self.input),
            Path::new(&self.output),
            progress,
        )
        .with_context(|| {
            format!(
                "failed to encode GIF from {} to {}",
                Path::new(&self.input).display(),
                Path::new(&self.output).display()
            )
        })
        .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        GifEncodeResult::from_stats(output)
    }
}

#[napi(js_name = "encodeGif", ts_return_type = "Promise<GifEncodeResult>")]
pub fn encode_gif(
    input: String,
    output: String,
    options: Option<GifEncodeOptions>,
    on_progress: Option<Function<ProgressEvent, ()>>,
) -> napi::Result<AsyncTask<EncodeGifTask>> {
    let config = to_encode_config(options).map_err(to_napi_error)?;

    let progress_callback = on_progress
        .map(|callback| callback.build_threadsafe_function::<ProgressEvent>().build())
        .transpose()?;

    Ok(AsyncTask::new(EncodeGifTask {
        input,
        output,
        config,
        progress_callback,
    }))
}

impl GifEncodeResult {
    fn from_stats(stats: EncodeStats) -> napi::Result<Self> {
        let frame_count = u32::try_from(stats.frame_count)
            .with_context(|| format!("frame_count {} does not fit into u32", stats.frame_count))
            .map_err(to_napi_error)?;

        Ok(Self {
            file_size_bytes: stats.file_size_bytes as f64,
            frame_count,
            duration_ms: stats.duration_ms as f64,
            width: stats.width,
            height: stats.height,
        })
    }
}

fn to_encode_config(options: Option<GifEncodeOptions>) -> Result<EncodeConfig> {
    let mut config = EncodeConfig::default();

    if let Some(options) = options {
        if let Some(fps) = options.fps {
            config.fps = fps;
        }

        if let Some(width) = options.width {
            config.width = Some(width);
        }

        if let Some(height) = options.height {
            config.height = Some(height);
        }

        if let Some(quality) = options.quality {
            config.quality = quality;
        }

        if let Some(dither) = options.dither {
            config.dither = parse_dither(&dither)?;
        }

        if let Some(temporal_window) = options.temporal_window {
            config.temporal_window = usize::try_from(temporal_window)
                .context("temporal_window is too large for this platform")?;
        }

        if let Some(transparency_optimization) = options.transparency_optimization {
            config.transparency_optimization = transparency_optimization;
        }

        if let Some(verbose) = options.verbose {
            config.verbose = verbose;
        }
    }

    config.validate()?;
    Ok(config)
}

fn parse_dither(value: &str) -> Result<DitherMethod> {
    match value.to_ascii_lowercase().as_str() {
        "floyd-steinberg" | "floyd_steinberg" | "floydsteinberg" => {
            Ok(DitherMethod::FloydSteinberg)
        }
        "bayer" => Ok(DitherMethod::Bayer),
        "none" => Ok(DitherMethod::None),
        other => bail!(
            "invalid dither method '{}'; expected one of: floyd-steinberg, bayer, none",
            other
        ),
    }
}

fn to_napi_error(error: anyhow::Error) -> Error {
    Error::from_reason(format!("{error:#}"))
}
