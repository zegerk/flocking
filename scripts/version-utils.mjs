import { readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';

export const CHANGE_CATEGORIES = ['Added', 'Changed', 'Deprecated', 'Removed', 'Fixed', 'Security'];
const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function parseVersion(version) {
  const match = SEMVER_PATTERN.exec(version);
  if (!match) throw new Error(`invalid strict SemVer: ${version}`);
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]) };
}

export function isVersion(version) {
  return typeof version === 'string' && SEMVER_PATTERN.test(version);
}

export function bumpVersion(version, type) {
  const { major, minor, patch } = parseVersion(version);
  if (type === 'patch') return `${major}.${minor}.${patch + 1}`;
  if (type === 'minor') return `${major}.${minor + 1}.0`;
  if (type === 'major') return `${major + 1}.0.0`;
  throw new Error(`unknown release type: ${type}`);
}

export function formatJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

export function cargoTomlVersion(source) {
  const packageBlock = source.match(/^\[package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m);
  if (!packageBlock) throw new Error('Cargo.toml: missing root [package] block');
  const matches = [...packageBlock[1].matchAll(/^version\s*=\s*"([^"]+)"\s*$/gm)];
  if (matches.length !== 1) throw new Error('Cargo.toml: expected one package version');
  return matches[0][1];
}

export function replaceCargoTomlVersion(source, nextVersion) {
  parseVersion(nextVersion);
  const current = cargoTomlVersion(source);
  const packageStart = source.indexOf('[package]');
  const nextSection = source.indexOf('\n[', packageStart + '[package]'.length);
  const packageEnd = nextSection === -1 ? source.length : nextSection;
  const block = source.slice(packageStart, packageEnd);
  const replaced = block.replace(
    /^version\s*=\s*"[^"]+"\s*$/m,
    `version = "${nextVersion}"`,
  );
  if (replaced === block) throw new Error(`Cargo.toml: could not replace version ${current}`);
  return source.slice(0, packageStart) + replaced + source.slice(packageEnd);
}

export function cargoLockVersion(source) {
  const blocks = source.split(/(?=^\[\[package\]\]\s*$)/m);
  const flocking = blocks.filter((block) => /^name\s*=\s*"flocking"\s*$/m.test(block));
  if (flocking.length !== 1) throw new Error('Cargo.lock: expected one flocking package entry');
  const match = flocking[0].match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!match) throw new Error('Cargo.lock: flocking entry has no version');
  return match[1];
}

export function unreleasedBody(changelog) {
  const match = changelog.match(/^## \[Unreleased\]\s*$([\s\S]*?)(?=^## \[|(?![\s\S]))/m);
  if (!match) throw new Error('CHANGELOG.md: expected one [Unreleased] section');
  if ((changelog.match(/^## \[Unreleased\]\s*$/gm) ?? []).length !== 1) {
    throw new Error('CHANGELOG.md: expected one [Unreleased] section');
  }
  return match[1].trim();
}

export function hasCategorizedChanges(body) {
  return CHANGE_CATEGORIES.some((category) => {
    const heading = new RegExp(`^### ${category}\\s*$`, 'm');
    if (!heading.test(body)) return false;
    const section = body.match(new RegExp(`^### ${category}\\s*$([\\s\\S]*?)(?=^### |(?![\\s\\S]))`, 'm'));
    return Boolean(section?.[1].match(/^\s*-\s+\S/m));
  });
}

export function validateChangelog(changelog, currentVersion, { requireChanges = false } = {}) {
  parseVersion(currentVersion);
  const errors = [];
  let body = '';
  try {
    body = unreleasedBody(changelog);
  } catch (error) {
    errors.push(error.message);
  }
  if (!new RegExp(`^## \\[${escapeRegex(currentVersion)}\\](?: - \\d{4}-\\d{2}-\\d{2})?\\s*$`, 'm').test(changelog)) {
    errors.push(`CHANGELOG.md: missing current [${currentVersion}] section`);
  }
  const comparison = changelog.match(/^\[Unreleased\]:\s*(\S+)\s*$/m);
  if (!comparison || !comparison[1].includes(`/compare/v${currentVersion}...HEAD`)) {
    errors.push(`CHANGELOG.md: [Unreleased] must compare v${currentVersion}...HEAD`);
  }
  if (requireChanges && !hasCategorizedChanges(body)) {
    errors.push('CHANGELOG.md: [Unreleased] has no categorized bullet entries');
  }
  return errors;
}

export function promoteChangelog(changelog, currentVersion, nextVersion, date) {
  parseVersion(currentVersion);
  parseVersion(nextVersion);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error(`invalid release date: ${date}`);
  const body = unreleasedBody(changelog);
  if (!hasCategorizedChanges(body)) throw new Error('CHANGELOG.md: [Unreleased] has no categorized bullet entries');

  const promoted = changelog.replace(
    /^## \[Unreleased\]\s*$[\s\S]*?(?=^## \[)/m,
    `## [Unreleased]\n\n## [${nextVersion}] - ${date}\n\n${body}\n\n`,
  );
  if (promoted === changelog) throw new Error('CHANGELOG.md: could not promote [Unreleased]');

  const nextComparison = `[Unreleased]: https://github.com/zegerk/flocking/compare/v${nextVersion}...HEAD`;
  const releaseComparison = `[${nextVersion}]: https://github.com/zegerk/flocking/compare/v${currentVersion}...v${nextVersion}`;
  const withUnreleased = promoted.replace(/^\[Unreleased\]:.*$/m, nextComparison);
  if (withUnreleased === promoted) throw new Error('CHANGELOG.md: missing [Unreleased] comparison link');
  return `${withUnreleased.trimEnd()}\n${releaseComparison}\n`;
}

export async function readVersionState(root) {
  const [packageSource, lockSource, cargoToml, cargoLock, changelog] = await Promise.all([
    readFile(path.join(root, 'package.json'), 'utf8'),
    readFile(path.join(root, 'package-lock.json'), 'utf8'),
    readFile(path.join(root, 'Cargo.toml'), 'utf8'),
    readFile(path.join(root, 'Cargo.lock'), 'utf8'),
    readFile(path.join(root, 'CHANGELOG.md'), 'utf8'),
  ]);
  const packageJson = JSON.parse(packageSource);
  const packageLock = JSON.parse(lockSource);
  const version = packageJson.version;
  parseVersion(version);
  return {
    version,
    packageJson,
    packageLock,
    cargoToml,
    cargoLock,
    changelog,
    versions: {
      'package.json': version,
      'package-lock.json': packageLock.version,
      'package-lock.json#packages[""]': packageLock.packages?.['']?.version,
      'Cargo.toml': cargoTomlVersion(cargoToml),
      'Cargo.lock': cargoLockVersion(cargoLock),
    },
  };
}

export function stateErrors(state, options) {
  const errors = [];
  for (const [location, version] of Object.entries(state.versions)) {
    if (version !== state.version) errors.push(`${location}: expected ${state.version}, found ${version ?? 'missing'}`);
  }
  return errors.concat(validateChangelog(state.changelog, state.version, options));
}

export async function atomicWrite(filePath, contents) {
  const temporary = `${filePath}.tmp-${process.pid}-${Date.now()}`;
  await writeFile(temporary, contents, 'utf8');
  await rename(temporary, filePath);
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}