/**
 * Map the licensing server's error contract onto honest,
 * actionable user-facing copy (the dialog's typed states).
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
  if (error.startsWith("NetworkError") || error.includes("Failed to fetch"))
    return "Could not reach the activation server. Check your connection and try again.";
  return "Activation failed. Please try again.";
}
