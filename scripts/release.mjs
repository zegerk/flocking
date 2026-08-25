import { execFileSync } from 'node:child_process';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import {
  atomicWrite,
  bumpVersion,
  formatJson,
  promoteChangelog,
  readVersionState,
  replaceCargoTomlVersion,
  stateErrors,
} from './version-utils.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const arguments_ = process.argv.slice(2);
const dryRun = arguments_.includes('--dry-run');
const positional = arguments_.filter((argument) => argument !== '--dry-run');
const type = positional[0];

if (positional.length !== 1 || !['patch', 'minor', 'major'].includes(type)) {
  console.error('usage: npm run release -- <patch|minor|major> [--dry-run]');
  process.exit(1);
}

function git(...args) {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
}

function hasTag(tag) {
  try {
    execFileSync('git', ['show-ref', '--verify', '--quiet', `refs/tags/${tag}`], { cwd: root });
    return true;
  } catch (error) {
    if (error.status === 1) return false;
    throw error;
  }
}

let writesStarted = false;
let changedFiles = [];

try {
  if (git('branch', '--show-current') !== 'main') throw new Error('release requires branch main');
  if (git('status', '--porcelain')) throw new Error('release requires a clean worktree; commit development and changelog changes first');

  const state = await readVersionState(root);
  const errors = stateErrors(state, { requireChanges: true });
  if (errors.length) throw new Error(errors.join('\n'));

  const nextVersion = bumpVersion(state.version, type);
  const currentTag = `v${state.version}`;
  const nextTag = `v${nextVersion}`;
  if (!hasTag(currentTag)) {
    throw new Error(`missing baseline tag ${currentTag}; create and push the approved annotated bootstrap tag first`);
  }
  if (hasTag(nextTag)) throw new Error(`target tag ${nextTag} already exists locally`);

  const packageJson = { ...state.packageJson, version: nextVersion };
  const packageLock = {
    ...state.packageLock,
    version: nextVersion,
    packages: {
      ...state.packageLock.packages,
      '': { ...state.packageLock.packages[''], version: nextVersion },
    },
  };
  const cargoToml = replaceCargoTomlVersion(state.cargoToml, nextVersion);
  const date = new Date().toISOString().slice(0, 10);
  const changelog = promoteChangelog(state.changelog, state.version, nextVersion, date);
  const outputs = new Map([
    ['package.json', formatJson(packageJson)],
    ['package-lock.json', formatJson(packageLock)],
    ['Cargo.toml', cargoToml],
    ['CHANGELOG.md', changelog],
  ]);
  changedFiles = [...outputs.keys(), 'Cargo.lock'];

  console.log(`${dryRun ? 'would release' : 'releasing'} ${state.version} -> ${nextVersion} (${type})`);
  console.log(`files: ${changedFiles.join(', ')}`);
  if (dryRun) process.exit(0);

  writesStarted = true;
  for (const [file, contents] of outputs) await atomicWrite(new URL(`../${file}`, import.meta.url), contents);
  execFileSync('cargo', ['metadata', '--format-version', '1'], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
  execFileSync('npm', ['run', 'version:check'], { cwd: root, stdio: 'inherit' });
  execFileSync('cargo', ['test'], { cwd: root, stdio: 'inherit' });
  execFileSync('npm', ['run', 'test:js'], { cwd: root, stdio: 'inherit' });

  console.log(`release metadata ${nextTag} is validated; review the diff`);
  console.log(`git add ${changedFiles.join(' ')}`);
  console.log(`git commit -m "release: ${nextTag}"`);
  console.log(`git tag -a ${nextTag} -m "${nextTag}"`);
  console.log(`git push origin main ${nextTag}`);
  console.log('deploy separately with the existing deployment command');
} catch (error) {
  console.error(error.message);
  if (writesStarted) {
    console.error(`an unvalidated bump may remain in: ${changedFiles.join(', ')}`);
    console.error('review and recover those files explicitly; nothing was reverted');
  }
  process.exitCode = 1;
}