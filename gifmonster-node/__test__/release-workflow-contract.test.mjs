import assert from 'node:assert/strict';
import fsp from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

async function readWorkflow(fileName) {
  return fsp.readFile(path.join(repoRoot, '.github', 'workflows', fileName), 'utf8');
}

function extractJob(workflow, jobName) {
  const lines = workflow.split('\n');
  const marker = `  ${jobName}:`;
  const start = lines.findIndex((line) => line === marker);
  assert.notEqual(start, -1, `Expected workflow job "${jobName}" to exist`);

  let end = lines.length;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(lines[i])) {
      end = i;
      break;
    }
  }

  return lines.slice(start, end);
}

function findLine(lines, regex, label) {
  const idx = lines.findIndex((line) => regex.test(line));
  assert.notEqual(idx, -1, `Expected to find ${label}`);
  return idx;
}

test('ci workflow includes release verification and dry-run pack gate', async () => {
  const ci = await readWorkflow('gifmonster-node-ci.yml');
  const nodeAddonTestsJob = extractJob(ci, 'node-addon-tests');

  const verifyIndex = findLine(
    nodeAddonTestsJob,
    /^\s+run:\s+npm run release:verify\s*$/,
    'release:verify command in node-addon-tests job',
  );
  const packDryRunIndex = findLine(
    nodeAddonTestsJob,
    /^\s+run:\s+npm run pack:dry-run\s*$/,
    'pack:dry-run command in node-addon-tests job',
  );

  assert.ok(
    verifyIndex < packDryRunIndex,
    'Expected release:verify to run before pack:dry-run in CI workflow',
  );
});

test('publish workflow verifies release readiness before publish while retaining tag/version guard', async () => {
  const publish = await readWorkflow('gifmonster-node-publish.yml');
  const publishJob = extractJob(publish, 'publish');

  const verifyTagStepIndex = findLine(
    publishJob,
    /^\s+- name:\s+Verify release tag matches package version\s*$/,
    'tag/version guard step in publish job',
  );
  const verifyIndex = findLine(
    publishJob,
    /^\s+run:\s+npm run release:verify\s*$/,
    'release:verify command in publish job',
  );
  const prepublishIndex = findLine(
    publishJob,
    /^\s+- name:\s+Prepare npm packages\s*$/,
    'Prepare npm packages step',
  );
  const publishPlatformIndex = findLine(
    publishJob,
    /^\s+- name:\s+Publish platform packages\s*$/,
    'Publish platform packages step',
  );
  const publishMainIndex = findLine(
    publishJob,
    /^\s+- name:\s+Publish main package\s*$/,
    'Publish main package step',
  );

  assert.ok(
    verifyTagStepIndex < verifyIndex,
    'Expected tag/version guard to run before release:verify in publish job',
  );

  assert.ok(
    verifyIndex < prepublishIndex,
    'Expected release:verify to run before preparing npm packages',
  );
  assert.ok(
    verifyIndex < publishPlatformIndex,
    'Expected release:verify to run before publishing platform packages',
  );
  assert.ok(
    verifyIndex < publishMainIndex,
    'Expected release:verify to run before publishing main package',
  );
});
