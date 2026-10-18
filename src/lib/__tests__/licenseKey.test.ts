/**
 * License key normalization / shape / grouping tests (the activation
 * contract: DB-XXXXX-XXXXX-XXXXX-XXXXX, Crockford base32 body).
 */
import { describe, expect, it } from "vitest";
import {
  formatKeyInput,
  isKeyComplete,
  isValidKeyShape,
  KEY_DISPLAY_LEN,
  KEY_NORMALIZED_LEN,
  normalizeKey,
} from "../licenseKey";

const GOOD = "DB-TESTS-DEM00-PR000-C0DE0";
// Marker demo keys (all Crockford-safe bodies — they must pass shape
// validation before the mock's marker checks fire).
const NOT_FOUND = "DB-WR0NG-00000-00000-00000";
const EXPIRED = "DB-EXP1R-00000-00000-00000";
const REVOKED = "DB-REV0K-00000-00000-00000";
const TAKEN = "DB-TAKEN-00000-00000-00000";

describe("normalizeKey", () => {
  it("uppercases and strips separators", () => {
    expect(normalizeKey("db-tests-demo0-pro00-life0")).toBe("DBTESTSDEMO0PRO00LIFE0");
    expect(normalizeKey("DB TESTS.DEMO0-PRO00_LIFE0")).toBe("DBTESTSDEMO0PRO00LIFE0");
  });
  it("strips unicode and whitespace noise", () => {
    expect(normalizeKey("  DB—TESTS—DEMO0—PRO00—LIFE0 ")).toBe("DBTESTSDEMO0PRO00LIFE0");
  });
  it("leaves an already-normal key untouched", () => {
    expect(normalizeKey("DBTESTSDEMO0PRO00LIFE0")).toBe("DBTESTSDEMO0PRO00LIFE0");
  });
});

describe("isValidKeyShape", () => {
  it("accepts the demo keys with and without separators", () => {
    for (const k of [GOOD, NOT_FOUND, EXPIRED, REVOKED, TAKEN]) {
      expect(isValidKeyShape(k)).toBe(true);
      expect(isValidKeyShape(k.toLowerCase())).toBe(true);
    }
  });
  it("rejects wrong length", () => {
    expect(isValidKeyShape("DB-TESTS-DEM00-PR000")).toBe(false);
    expect(isValidKeyShape(`${GOOD}X`)).toBe(false);
  });
  it("rejects a non-DB prefix", () => {
    expect(isValidKeyShape("XXTESTSDEM00PR000C0DE0")).toBe(false);
  });
  it("rejects Crockford-excluded characters in the body", () => {
    // I/L/O/U are excluded from the base32 alphabet.
    expect(isValidKeyShape("DB-TESTI-DEM00-PR000-C0DE0")).toBe(false);
    expect(isValidKeyShape("DB-TESTL-DEM00-PR000-C0DE0")).toBe(false);
    expect(isValidKeyShape("DB-TESTO-DEM00-PR000-C0DE0")).toBe(false);
    expect(isValidKeyShape("DB-TESTU-DEM00-PR000-C0DE0")).toBe(false);
  });
  it("rejects empty / junk input", () => {
    expect(isValidKeyShape("")).toBe(false);
    expect(isValidKeyShape("--------")).toBe(false);
    expect(isValidKeyShape("DB")).toBe(false);
  });
});

describe("formatKeyInput", () => {
  it("groups the typed body into 5-char blocks", () => {
    expect(formatKeyInput("DBTESTSDEMO")).toBe("DB-TESTS-DEMO");
  });
  it("reformats pasted separated keys identically", () => {
    expect(formatKeyInput(GOOD)).toBe(GOOD);
  });
  it("keeps partial groups without padding", () => {
    expect(formatKeyInput("DB-TESTS-D")).toBe("DB-TESTS-D");
    expect(formatKeyInput("DBTESTSD")).toBe("DB-TESTS-D");
  });
  it("echoes raw input until the DB prefix lands", () => {
    expect(formatKeyInput("")).toBe("");
    expect(formatKeyInput("D")).toBe("D");
    expect(formatKeyInput("DB")).toBe("DB");
    expect(formatKeyInput("X12")).toBe("X12");
  });
  it("clamps overflow beyond the 22-char key", () => {
    expect(formatKeyInput(`${GOOD}EXTRAEXTRA`).length).toBe(KEY_DISPLAY_LEN);
  });
});

describe("isKeyComplete", () => {
  it("mirrors shape validity (the CTA arming rule)", () => {
    expect(isKeyComplete(GOOD)).toBe(true);
    expect(isKeyComplete("DB-TESTS")).toBe(false);
    expect(isKeyComplete("")).toBe(false);
  });
});

describe("key length constants", () => {
  it("keeps the documented contract (22 normalized / 26 displayed)", () => {
    expect(KEY_NORMALIZED_LEN).toBe(22);
    expect(KEY_DISPLAY_LEN).toBe(26);
  });
});
