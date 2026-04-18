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

## Runtime Requirements

- Video input requires `ffmpeg` and `ffprobe` available on your PATH.
- Directory input (PNG/JPEG frames) does not require ffmpeg.

## More Information

- Repository and full docs: https://github.com/garrettsiegel/gifmonster

## License

MIT OR Apache-2.0
