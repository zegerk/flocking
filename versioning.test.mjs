import test from 'node:test';
import assert from 'node:assert/strict';

import {
  bumpVersion,
  cargoLockVersion,
  cargoTomlVersion,
  hasCategorizedChanges,
  parseVersion,
  promoteChangelog,
  replaceCargoTomlVersion,
  stateErrors,
  validateChangelog,
} from './scripts/version-utils.mjs';
import { formatBuildVersion } from './version-display.mjs';

const CHANGELOG = `# Changelog

## [Unreleased]

### Added

- Add release tooling.

## [0.1.12] - 2026-08-25

### Added

- Add 24D simulation.

[Unreleased]: https://github.com/zegerk/flocking/compare/v0.1.12...HEAD
[0.1.12]: https://github.com/zegerk/flocking/tree/v0.1.12
`;

test('strict versions parse and increment', () => {
  assert.deepEqual(parseVersion('0.1.12'), { major: 0, minor: 1, patch: 12 });
  assert.equal(bumpVersion('0.1.12', 'patch'), '0.1.13');
  assert.equal(bumpVersion('0.1.12', 'minor'), '0.2.0');
  assert.equal(bumpVersion('0.1.12', 'major'), '1.0.0');
  for (const invalid of ['01.2.3', '1.02.3', '1.2', 'v1.2.3', '1.2.3-beta']) {
    assert.throws(() => parseVersion(invalid), /invalid strict SemVer/);
  }
});

test('Cargo versions are scoped to the root package', () => {
  const toml = `[package]\nname = "flocking"\nversion = "0.1.12"\n\n[dependencies.foo]\nversion = "4"\n`;
  assert.equal(cargoTomlVersion(toml), '0.1.12');
  assert.match(replaceCargoTomlVersion(toml, '0.2.0'), /version = "0\.2\.0"/);
  assert.match(replaceCargoTomlVersion(toml, '0.2.0'), /version = "4"/);
  assert.throws(() => cargoTomlVersion('[dependencies]\nfoo = "1"\n'), /missing root/);

  const lock = `[[package]]\nname = "foo"\nversion = "9"\n\n[[package]]\nname = "flocking"\nversion = "0.1.12"\n`;
  assert.equal(cargoLockVersion(lock), '0.1.12');
});

test('changelog promotion creates release and comparison links', () => {
  const promoted = promoteChangelog(CHANGELOG, '0.1.12', '0.2.0', '2026-08-25');
  assert.match(promoted, /^## \[Unreleased\]\s*$/m);
  assert.match(promoted, /^## \[0\.2\.0\] - 2026-08-25$/m);
  assert.match(promoted, /\[Unreleased\]: .*v0\.2\.0\.\.\.HEAD/);
  assert.match(promoted, /\[0\.2\.0\]: .*v0\.1\.12\.\.\.v0\.2\.0/);
  assert.deepEqual(validateChangelog(promoted, '0.2.0'), []);
});

test('changelog rejects empty and malformed release content', () => {
  assert.equal(hasCategorizedChanges('### Added\n\n- One item.'), true);
  assert.equal(hasCategorizedChanges('### Added\n'), false);
  assert.throws(
    () => promoteChangelog(CHANGELOG.replace('- Add release tooling.', ''), '0.1.12', '0.2.0', '2026-08-25'),
    /no categorized bullet/,
  );
  assert.ok(validateChangelog(CHANGELOG, '0.1.13').length >= 2);
  assert.match(
    validateChangelog(CHANGELOG.replace('## [0.1.12]', '## [Unreleased]\n\n## [0.1.12]'), '0.1.12')[0],
    /expected one \[Unreleased\]/,
  );
});

test('version state reports every mismatched metadata location', () => {
  const errors = stateErrors({
    version: '0.1.12',
    versions: {
      'package.json': '0.1.12',
      'package-lock.json': '0.1.11',
      'Cargo.toml': '0.2.0',
    },
    changelog: CHANGELOG,
  });
  assert.deepEqual(errors, [
    'package-lock.json: expected 0.1.12, found 0.1.11',
    'Cargo.toml: expected 0.1.12, found 0.2.0',
  ]);
});

test('build metadata formatter accepts only complete valid metadata', () => {
  assert.deepEqual(formatBuildVersion({
    version: '0.1.12',
    commit: 'abc123',
    dirty: true,
    builtAt: '2026-08-25T12:34:56.000Z',
  }), {
    label: 'v0.1.12',
    description: 'Version 0.1.12, commit abc123 (dirty), built 2026-08-25T12:34:56.000Z',
  });
  assert.equal(formatBuildVersion({ version: '01.1.12' }), null);
  assert.equal(formatBuildVersion({
    version: '0.1.12', commit: '', dirty: false, builtAt: 'not-a-date',
  }), null);
});