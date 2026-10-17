// Versions as Lockra and its extension name them (release-please's x.y.z): which is older, so that
// the extension and Settings › 浏览器 can tell one left behind.

/** A version's parts as numbers; null when it is not dot-separated numbers. */
function parts(version: string): number[] | null {
  return /^\d+(\.\d+)*$/.test(version) ? version.split(".").map(Number) : null;
}

/** `a` before `b` (negative), the same (0) or after (positive), part by part as numbers (a missing
 *  part counts 0); null when either is not dot-separated numbers. */
export function compareVersions(a: string, b: string): number | null {
  const left = parts(a);
  const right = parts(b);
  if (!left || !right) return null;
  for (let i = 0; i < Math.max(left.length, right.length); i += 1) {
    const difference = (left[i] ?? 0) - (right[i] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

/** `version` is known to be older than `than`. */
export function olderThan(version: string | null, than: string | null): boolean {
  if (version === null || than === null) return false;
  const order = compareVersions(version, than);
  return order !== null && order < 0;
}
