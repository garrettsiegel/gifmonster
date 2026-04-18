import assert from 'node:assert/strict';
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { encodeGif } = require('../index.js');
const { PNG } = require('pngjs');

function createSolidPngBuffer(r, g, b, a = 255) {
  const png = new PNG({ width: 1, height: 1 });
  png.data[0] = r;
  png.data[1] = g;
  png.data[2] = b;
  png.data[3] = a;
  return PNG.sync.write(png);
}

async function createFrameInputDir() {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'gifmonster-node-test-'));
  await fsp.writeFile(path.join(dir, '0001.png'), createSolidPngBuffer(255, 0, 0));
  await fsp.writeFile(path.join(dir, '0002.png'), createSolidPngBuffer(0, 0, 255));
  return dir;
}

test('encodeGif returns stats and writes output', async () => {
  const inputDir = await createFrameInputDir();
  const outputPath = path.join(inputDir, 'out.gif');

  try {
    const stats = await encodeGif(inputDir, outputPath, {
      fps: 10,
      quality: 85,
      dither: 'floyd-steinberg',
    });

    assert.ok(fs.existsSync(outputPath), 'expected output GIF file to exist');
    assert.ok(stats.fileSizeBytes > 0, 'expected output GIF to be non-empty');
    assert.equal(stats.frameCount, 2);
    assert.equal(stats.width, 1);
    assert.equal(stats.height, 1);
  } finally {
    await fsp.rm(inputDir, { recursive: true, force: true });
  }
});

test('encodeGif invokes progress callback', async () => {
  const inputDir = await createFrameInputDir();
  const outputPath = path.join(inputDir, 'progress.gif');
  const events = [];

  try {
    await encodeGif(
      inputDir,
      outputPath,
      { fps: 8, quality: 80 },
      (event) => {
        events.push(event.event);
      },
    );

    assert.ok(events.includes('stage'));
    assert.ok(events.includes('length'));
    assert.ok(events.includes('progress'));
  } finally {
    await fsp.rm(inputDir, { recursive: true, force: true });
  }
});

test('encodeGif rejects for missing input path', async () => {
  const missingPath = path.join(os.tmpdir(), 'gifmonster-node-missing-input');
  const outputPath = path.join(os.tmpdir(), 'gifmonster-node-missing-output.gif');

  await assert.rejects(
    () => encodeGif(missingPath, outputPath, null),
    /input path does not exist/i,
  );
});
