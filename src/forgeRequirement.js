export function resolveForgeRequirement(requirement, versions = []) {
  const raw = String(requirement || '').replace(/^Forge\s*/i, '').trim();
  if (!raw) return '';

  const available = versions.map((version) => String(version.id || version.value || ''));
  const exact = available.find((version) => version === raw);
  if (exact) return exact;

  const build = raw.match(/^#?(\d+)$/)?.[1];
  if (build) {
    const matches = available.filter((version) => version.split(/[.-]/).at(-1) === build);
    if (matches.length === 1) return matches[0];
  }

  return raw;
}
