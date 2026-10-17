/**
 * License key normalization/format/completeness (pure, unit-testable;
 * shared by the activation dialog + the mock backend + the tour).
 *
 * Format (DiskGenie licensing architecture, adapted to the DW prefix):
 *   DW-XXXXX-XXXXX-XXXXX-XXXXX  — Crockford base32 (no I/L/O/U),
 *   20 random characters = 100 bits of entropy. Normalization:
 *   uppercase, strip non-alphanumerics → 22 chars.
 */

export const KEY_PREFIX = "DW";
export const KEY_GROUPS = 4;
export const KEY_GROUP_LEN = 5;
/** Normalized length: 2 (prefix) + 4×5 (groups). */
export const KEY_NORMALIZED_LEN = KEY_PREFIX.length + KEY_GROUPS * KEY_GROUP_LEN;
/** Display length including separators: DW-XXXXX-XXXXX-XXXXX-XXXXX. */
export const KEY_DISPLAY_LEN = KEY_NORMALIZED_LEN + KEY_GROUPS; // 3 dashes + 1 prefix dash

const CROCKFORD = /^[0-9A-Z]+$/;

/** Uppercase + strip everything that is not alphanumeric. */
export function normalizeKey(raw: string): string {
  return raw.toUpperCase().replace(/[^0-9A-Z]/g, "");
}

/** True when the normalized form is exactly 22 Crockford-safe chars. */
export function isValidKeyShape(raw: string): boolean {
  const n = normalizeKey(raw);
  return n.length === KEY_NORMALIZED_LEN && n.startsWith(KEY_PREFIX) && CROCKFORD.test(n);
}

/** Pretty-print a (possibly partial) normalized key into the grouped
 * display form, so typing naturally lands on DW-XXXXX-XXXXX-…. */
export function formatKeyInput(raw: string): string {
  const n = normalizeKey(raw).slice(0, KEY_NORMALIZED_LEN);
  const body = n.startsWith(KEY_PREFIX) ? n.slice(KEY_PREFIX.length) : n;
  const groups: string[] = [];
  for (let i = 0; i < body.length && groups.length < KEY_GROUPS; i += KEY_GROUP_LEN) {
    groups.push(body.slice(i, i + KEY_GROUP_LEN));
  }
  const joined = groups.join("-");
  return groups.length ? `${KEY_PREFIX}-${joined}` : n ? `${KEY_PREFIX}` : "";
}

/** The activation CTA is only armed when the key is shape-complete. */
export function isKeyComplete(raw: string): boolean {
  return isValidKeyShape(raw);
}
