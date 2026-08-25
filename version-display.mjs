const STRICT_SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const ISO_TIMESTAMP = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;

export function formatBuildVersion(metadata) {
  if (!metadata || typeof metadata !== 'object') return null;
  if (typeof metadata.version !== 'string' || !STRICT_SEMVER.test(metadata.version)) return null;
  if (typeof metadata.commit !== 'string' || metadata.commit.length === 0) return null;
  if (typeof metadata.dirty !== 'boolean') return null;
  if (typeof metadata.builtAt !== 'string' || !ISO_TIMESTAMP.test(metadata.builtAt)) return null;
  if (Number.isNaN(Date.parse(metadata.builtAt))) return null;

  const dirty = metadata.dirty ? ' (dirty)' : '';
  return {
    label: `v${metadata.version}`,
    description: `Version ${metadata.version}, commit ${metadata.commit}${dirty}, built ${metadata.builtAt}`,
  };
}