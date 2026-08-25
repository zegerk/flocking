import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { atomicWrite, formatJson, readVersionState, stateErrors } from './version-utils.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const outputIndex = process.argv.indexOf('--output');
if (outputIndex !== -1 && (!process.argv[outputIndex + 1] || process.argv[outputIndex + 2])) {
  console.error('usage: node scripts/write-build-version.mjs [--output <directory>]');
  process.exit(1);
}
const output = outputIndex === -1 ? process.env.TRUNK_STAGING_DIR : process.argv[outputIndex + 1];
if (!output) {
  console.error('TRUNK_STAGING_DIR is required outside explicit --output test runs');
  process.exit(1);
}

function git(args, fallback) {
  try {
    return execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    return fallback;
  }
}

try {
  const state = await readVersionState(root);
  const errors = stateErrors(state);
  if (errors.length) throw new Error(errors.join('\n'));

  const commit = process.env.CF_PAGES_COMMIT_SHA
    || process.env.GITHUB_SHA
    || git(['rev-parse', '--short=12', 'HEAD'], 'unknown');
  const dirty = git(['status', '--porcelain'], '') !== '';
  const metadata = {
    version: state.version,
    commit,
    dirty,
    builtAt: new Date().toISOString(),
  };
  await mkdir(output, { recursive: true });
  const destination = path.join(output, 'version.json');
  await atomicWrite(destination, formatJson(metadata));
  console.log(`wrote ${destination}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}