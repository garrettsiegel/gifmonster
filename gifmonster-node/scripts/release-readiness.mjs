import fsp from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PLATFORM_TARGETS = [
  { dir: 'darwin-arm64', packageName: 'gifmonster-darwin-arm64' },
  { dir: 'darwin-x64', packageName: 'gifmonster-darwin-x64' },
  { dir: 'linux-x64-gnu', packageName: 'gifmonster-linux-x64-gnu' },
  { dir: 'linux-x64-musl', packageName: 'gifmonster-linux-x64-musl' },
  { dir: 'win32-x64-msvc', packageName: 'gifmonster-win32-x64-msvc' },
];

async function readJson(filePath, label) {
  let raw;
  try {
    raw = await fsp.readFile(filePath, 'utf8');
  } catch (error) {
    throw new Error(
      `${label} could not be read at ${filePath}: ${error.message}`,
      { cause: error },
    );
  }

  try {
    return JSON.parse(raw);
  } catch (error) {
    throw new Error(
      `${label} is not valid JSON at ${filePath}: ${error.message}`,
      { cause: error },
    );
  }
}

function pathEqual(leftPath, rightPath) {
  return path.resolve(leftPath) === path.resolve(rightPath);
}

function isValidSemver(value) {
  return /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(
    value,
  );
}

export async function verifyReleaseReadiness(rootDir) {
  const errors = [];

  let mainPkg;
  try {
    mainPkg = await readJson(
      path.join(rootDir, 'package.json'),
      'Main package manifest',
    );
  } catch (error) {
    return {
      ok: false,
      errors: [error.message],
      rootVersion: null,
      checkedTargets: 0,
    };
  }

  const rootVersion = mainPkg.version;
  if (typeof rootVersion !== 'string' || rootVersion.trim() === '') {
    errors.push('Main package manifest has an invalid version field');
  } else if (!isValidSemver(rootVersion)) {
    errors.push(
      `Main package manifest version is not valid semver: ${rootVersion}`,
    );
  }

  try {
    const lockfile = await readJson(
      path.join(rootDir, 'package-lock.json'),
      'Main package lockfile',
    );

    if (lockfile.version !== rootVersion) {
      errors.push(
        `Main package lockfile version mismatch: ${lockfile.version ?? '(missing)'}, expected ${rootVersion}`,
      );
    }

    const lockfileRootPackageVersion = lockfile.packages?.['']?.version;
    if (lockfileRootPackageVersion !== rootVersion) {
      errors.push(
        `Main package lockfile packages[""].version mismatch: ${lockfileRootPackageVersion ?? '(missing)'}, expected ${rootVersion}`,
      );
    }
  } catch (error) {
    errors.push(error.message);
  }

  const optionalDependencies = mainPkg.optionalDependencies ?? {};

  for (const target of PLATFORM_TARGETS) {
    const expectedOptionalDepVersion = optionalDependencies[target.packageName];
    if (expectedOptionalDepVersion !== rootVersion) {
      errors.push(
        `Main package optionalDependencies mismatch: ${target.packageName}=${expectedOptionalDepVersion ?? '(missing)'}, expected ${rootVersion}`,
      );
    }

    const filePath = path.join(rootDir, 'npm', target.dir, 'package.json');
    let pkg;
    try {
      pkg = await readJson(filePath, `Platform package ${target.packageName}`);
    } catch (error) {
      errors.push(error.message);
      continue;
    }

    if (pkg.name !== target.packageName) {
      errors.push(
        `Platform package name mismatch in ${target.dir}: found ${pkg.name}, expected ${target.packageName}`,
      );
    }

    if (pkg.version !== rootVersion) {
      errors.push(
        `Version mismatch: ${target.packageName}=${pkg.version}, expected ${rootVersion}`,
      );
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    rootVersion,
    checkedTargets: PLATFORM_TARGETS.length,
  };
}

async function main() {
  const rootDir = process.cwd();
  const result = await verifyReleaseReadiness(rootDir);

  if (!result.ok) {
    console.error('Release readiness failed:');
    for (const err of result.errors) {
      console.error(`- ${err}`);
    }
    process.exit(1);
  }

  console.log(`Release readiness passed for version ${result.rootVersion}`);
}

const isMainModule =
  process.argv[1] &&
  pathEqual(fileURLToPath(import.meta.url), process.argv[1]);

if (isMainModule) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
