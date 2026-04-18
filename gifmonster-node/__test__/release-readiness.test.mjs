import assert from 'node:assert/strict';
import fsp from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { verifyReleaseReadiness } from '../scripts/release-readiness.mjs';

const PLATFORM_PACKAGES = [
  'gifmonster-darwin-arm64',
  'gifmonster-darwin-x64',
  'gifmonster-linux-x64-gnu',
  'gifmonster-linux-x64-musl',
  'gifmonster-win32-x64-msvc',
];

async function writeJson(filePath, value) {
  await fsp.mkdir(path.dirname(filePath), { recursive: true });
  await fsp.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

async function createFixture({ rootVersion, overridePlatformVersion }) {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'gifmonster-release-'));

  await writeJson(path.join(dir, 'package.json'), {
    name: 'gifmonster',
    version: rootVersion,
    optionalDependencies: Object.fromEntries(
      PLATFORM_PACKAGES.map((name) => [name, rootVersion]),
    ),
  });

  await writeJson(path.join(dir, 'package-lock.json'), {
    name: 'gifmonster',
    lockfileVersion: 3,
    version: rootVersion,
    packages: {
      '': {
        name: 'gifmonster',
        version: rootVersion,
      },
    },
  });

  for (const name of PLATFORM_PACKAGES) {
    const version =
      name === 'gifmonster-linux-x64-musl' && overridePlatformVersion
        ? overridePlatformVersion
        : rootVersion;

    await writeJson(
      path.join(dir, 'npm', name.replace('gifmonster-', ''), 'package.json'),
      {
        name,
        version,
      },
    );
  }

  return dir;
}

test('verifyReleaseReadiness passes when versions are consistent', async () => {
  const fixture = await createFixture({ rootVersion: '0.2.0' });

  try {
    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, true);
    assert.equal(result.errors.length, 0);
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});

test('verifyReleaseReadiness fails on platform package version mismatch', async () => {
  const fixture = await createFixture({
    rootVersion: '0.2.0',
    overridePlatformVersion: '0.1.9',
  });

  try {
    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, false);
    assert.match(result.errors.join('\n'), /version mismatch/i);
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});

test('verifyReleaseReadiness fails when a platform package manifest is missing', async () => {
  const fixture = await createFixture({ rootVersion: '0.2.0' });

  try {
    await fsp.rm(path.join(fixture, 'npm', 'linux-x64-musl', 'package.json'));

    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, false);
    assert.match(
      result.errors.join('\n'),
      /could not be read.+linux-x64-musl\/package\.json/i,
    );
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});

test('verifyReleaseReadiness fails when a platform package manifest has invalid JSON', async () => {
  const fixture = await createFixture({ rootVersion: '0.2.0' });

  try {
    await fsp.writeFile(
      path.join(fixture, 'npm', 'darwin-arm64', 'package.json'),
      '{ this is invalid json',
    );

    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, false);
    assert.match(
      result.errors.join('\n'),
      /not valid json.+darwin-arm64\/package\.json/i,
    );
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});

test('verifyReleaseReadiness fails when root version is not valid semver', async () => {
  const fixture = await createFixture({ rootVersion: 'version-one' });

  try {
    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, false);
    assert.match(
      result.errors.join('\n'),
      /not valid semver: version-one/i,
    );
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});

test('verifyReleaseReadiness fails when lockfile version does not match root version', async () => {
  const fixture = await createFixture({ rootVersion: '0.2.0' });

  try {
    const lockfilePath = path.join(fixture, 'package-lock.json');
    const lockfile = JSON.parse(await fsp.readFile(lockfilePath, 'utf8'));
    lockfile.version = '0.1.9';
    lockfile.packages[''].version = '0.1.9';
    await writeJson(lockfilePath, lockfile);

    const result = await verifyReleaseReadiness(fixture);
    assert.equal(result.ok, false);
    assert.match(
      result.errors.join('\n'),
      /lockfile version mismatch: 0\.1\.9, expected 0\.2\.0/i,
    );
    assert.match(
      result.errors.join('\n'),
      /packages\[""\]\.version mismatch: 0\.1\.9, expected 0\.2\.0/i,
    );
  } finally {
    await fsp.rm(fixture, { recursive: true, force: true });
  }
});
