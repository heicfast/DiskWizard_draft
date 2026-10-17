/**
 * The licensing error contract → user-facing copy mapping. Every typed
 * refusal state the activation server can return must land as honest,
 * actionable copy (never a raw code, never a generic lie).
 */
import { describe, expect, it } from "vitest";
import { userFacingError } from "../userFacingError";

describe("userFacingError", () => {
  it("maps every typed refusal to actionable copy", () => {
    expect(userFacingError("KEY_NOT_FOUND. nope")).toBe(
      "Invalid license key. Check the code and try again.",
    );
    expect(userFacingError("LICENSE_EXPIRED. 2026-01-01")).toBe(
      "Your yearly license has expired. Renew to continue recovering.",
    );
    expect(userFacingError("KEY_REVOKED. x")).toBe(
      "This license has been revoked. Contact support for help.",
    );
    expect(userFacingError("KEY_REFUNDED. x")).toBe(
      "This license was refunded and is no longer active.",
    );
    expect(userFacingError("DEVICE_SLOT_TAKEN. x")).toBe(
      "This license is already active on another device. Contact support to move it.",
    );
    expect(userFacingError("DEVICE_MISMATCH. x")).toBe(
      "Already activated on another device of this platform.",
    );
    expect(userFacingError("RATE_LIMITED. x")).toBe(
      "Too many attempts. Please wait a moment and try again.",
    );
    expect(userFacingError("BAD_REQUEST. shape")).toBe(
      "The license key format is invalid.",
    );
  });

  it("maps transport failures to the connection copy", () => {
    expect(userFacingError("NetworkError: timeout")).toBe(
      "Could not reach the activation server. Check your connection and try again.",
    );
    expect(userFacingError("TypeError: Failed to fetch")).toBe(
      "Could not reach the activation server. Check your connection and try again.",
    );
  });

  it("falls back to honest generic copy for unknown codes", () => {
    expect(userFacingError("SOMETHING_ELSE. ???")).toBe("Activation failed. Please try again.");
  });

  it("treats null/empty as no error", () => {
    expect(userFacingError(null)).toBeNull();
    expect(userFacingError("")).toBeNull();
  });
});
