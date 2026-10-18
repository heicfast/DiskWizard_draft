/**
 * Map the licensing server's error contract onto honest,
 * actionable user-facing copy (the dialog's typed states). Every
 * marker the backend command layer emits (marker_for / user_facing in
 * commands/real/license.rs) must land as honest, actionable copy —
 * never a raw code, never a generic lie ("Activation failed" for a
 * verifiable transport or verification failure is a silent failure
 * from the user's point of view).
 */
export function userFacingError(error: string | null): string | null {
  if (!error) return null;
  if (error.startsWith("KEY_NOT_FOUND")) return "Invalid license key. Check the code and try again.";
  if (error.startsWith("LICENSE_EXPIRED")) return "Your yearly license has expired. Renew to continue recovering.";
  if (error.startsWith("KEY_REVOKED")) return "This license has been revoked. Contact support for help.";
  if (error.startsWith("KEY_REFUNDED")) return "This license was refunded and is no longer active.";
  if (error.startsWith("DEVICE_SLOT_TAKEN"))
    return "This license is already active on another device. Contact support to move it.";
  if (error.startsWith("DEVICE_MISMATCH")) return "Already activated on another device of this platform.";
  if (error.startsWith("RATE_LIMITED")) return "Too many attempts. Please wait a moment and try again.";
  if (error.startsWith("BAD_REQUEST")) return "The license key format is invalid.";
  if (error.startsWith("SPOOFED"))
    return "The license server's response could not be verified. Reconnect and try again.";
  if (error.startsWith("NETWORK"))
    return "Could not reach the activation server. Check your connection and try again.";
  if (error.startsWith("SERVER_ERROR"))
    return "The license server had a problem. Try again in a moment.";
  if (error.startsWith("NetworkError") || error.includes("Failed to fetch"))
    return "Could not reach the activation server. Check your connection and try again.";
  return "Activation failed. Please try again.";
}
