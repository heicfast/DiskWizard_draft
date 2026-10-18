/**
 * License key normalization/format/completeness (pure, unit-testable;
 * shared by the activation dialog + the mock backend + the tour).
 *
 * Format (DiskGenie licensing architecture — DB prefix while the
 * DiskGenie server validates our activations, per §8 config module
 * `dw-license/src/config.rs` KEY_PREFIX):
 *   DB-XXXXX-XXXXX-XXXXX-XXXXX  — Crockford base32 (no I/L/O/U),
 *   20 random characters = 100 bits of entropy. Normalization:
 *   uppercase, strip non-alphanumerics → 22 chars.
 */

export const KEY_PREFIX = "DB";
export const KEY_GROUPS = 4;
export const KEY_GROUP_LEN = 5;
/** Normalized length: 2 (prefix) + 4×5 (groups). */
export const KEY_NORMALIZED_LEN = KEY_PREFIX.length + KEY_GROUPS * KEY_GROUP_LEN;
/** Display length including separators: DW-XXXXX-XXXXX-XXXXX-XXXXX. */
export const KEY_DISPLAY_LEN = KEY_NORMALIZED_LEN + KEY_GROUPS; // 3 dashes + 1 prefix dash

/** Crockford base32 alphabet (excludes I, L, O, U — no confusables).
 * The key prefix is exempt (checked separately); the 20-char body
 * must draw only from this alphabet — same contract as the donor
 * licensing architecture and the server's normalized-hash form. */
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** Uppercase + strip everything that is not alphanumeric. */
export function normalizeKey(raw: string): string {
  return raw.toUpperCase().replace(/[^0-9A-Z]/g, "");
}

/** True when the normalized form is exactly KEY_PREFIX + 20 Crockford
 * chars (see the §8 config module note above). */
export function isValidKeyShape(raw: string): boolean {
  const n = normalizeKey(raw);
  return (
    n.length === KEY_NORMALIZED_LEN &&
    n.startsWith(KEY_PREFIX) &&
    n
      .slice(KEY_PREFIX.length)
      .split("")
      .every((c) => CROCKFORD.includes(c))
  );
}

/** Pretty-print a (possibly partial) normalized key into the grouped
 * display form, so typing naturally lands on DB-XXXXX-XXXXX-….
 * Raw input is echoed as-typed until the DB prefix lands (never paint
 * a phantom "DB-" over what the user is still typing). */
export function formatKeyInput(raw: string): string {
  const n = normalizeKey(raw).slice(0, KEY_NORMALIZED_LEN);
  if (!n.startsWith(KEY_PREFIX)) return n;
  const body = n.slice(KEY_PREFIX.length);
  const groups: string[] = [];
  for (let i = 0; i < body.length && groups.length < KEY_GROUPS; i += KEY_GROUP_LEN) {
    groups.push(body.slice(i, i + KEY_GROUP_LEN));
  }
  const joined = groups.join("-");
  return groups.length ? `${KEY_PREFIX}-${joined}` : KEY_PREFIX;
}

/** The activation CTA is only armed when the key is shape-complete. */
export function isKeyComplete(raw: string): boolean {
  return isValidKeyShape(raw);
}
