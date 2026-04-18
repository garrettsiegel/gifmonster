const { platform, arch } = process;

const targets = {
  'darwin-arm64': 'gifmonster-darwin-arm64',
  'darwin-x64': 'gifmonster-darwin-x64',
  'linux-x64': 'gifmonster-linux-x64-gnu',
  'win32-x64': 'gifmonster-win32-x64-msvc',
};

function isMusl() {
  if (platform !== 'linux') {
    return false;
  }

  const report = process.report && typeof process.report.getReport === 'function'
    ? process.report.getReport()
    : null;
  const glibcVersion = report?.header?.glibcVersionRuntime;
  return !glibcVersion;
}

function requireNativeBinding() {
  try {
    return require('./index.node');
  } catch (_) {
    // Fall through and load from platform package.
  }

  if (platform === 'linux' && arch === 'x64' && isMusl()) {
    return require('gifmonster-linux-x64-musl');
  }

  const key = `${platform}-${arch}`;
  const packageName = targets[key];
  if (!packageName) {
    throw new Error(`Unsupported platform: ${key}`);
  }

  return require(packageName);
}

module.exports = requireNativeBinding();
