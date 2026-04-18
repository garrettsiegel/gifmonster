# gifmonster

High-quality GIF encoding for Node.js with native Rust performance.

## Install

```bash
npm install gifmonster
```

Prebuilt native binaries are published for:

- macOS: arm64, x64
- Linux: x64 (glibc), x64 (musl)
- Windows: x64

## Quick Start

```js
const { encodeGif } = require('gifmonster');

const stats = await encodeGif('./frames', './out.gif', {
  fps: 12,
  quality: 85,
  dither: 'floyd-steinberg',
});

console.log(stats);
```

## API

### encodeGif(input, output, options?, onProgress?)

Returns: `Promise<GifEncodeResult>`

- `input` (`string`): Path to a directory of PNG/JPEG files or a video file.
- `output` (`string`): Path for the output GIF.
- `options` (`GifEncodeOptions`, optional): Encoder options.
- `onProgress` (`(event: ProgressEvent) => void`, optional): Progress callback.

## Options

All options are optional.

- `fps` (`number`, default: `10`): Output frame rate.
- `width` (`number`): Output width in pixels.
- `height` (`number`): Output height in pixels.
- `quality` (`number`, `1-100`, default: `90`): Higher quality usually means larger files.
- `dither` (`'floyd-steinberg' | 'bayer' | 'none'`, default: `'floyd-steinberg'`)
- `temporalWindow` (`number`, default: `3`): Number of neighboring frames for palette smoothing.
- `transparencyOptimization` (`boolean`, default: `true`): Reuse unchanged pixels as transparency.
- `verbose` (`boolean`, default: `false`)

## Progress Callback

If provided, `onProgress` receives events like:

```js
{ event: 'stage', stage: 'Extracting frames' }
{ event: 'length', length: 120 }
{ event: 'progress', delta: 1 }
{ event: 'finish', message: 'Done' }
```

## Result

`encodeGif` resolves to:

```ts
type GifEncodeResult = {
  fileSizeBytes: number;
  frameCount: number;
  durationMs: number;
  width: number;
  height: number;
};
```

## Runtime Requirements

- Video input requires `ffmpeg` and `ffprobe` available on your PATH.
- Directory input (PNG/JPEG frames) does not require ffmpeg.

## License

MIT OR Apache-2.0
