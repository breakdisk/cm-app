/**
 * Validate a payment checkout URL handed back by the server before navigating
 * to it. `location.assign("javascript:...")` runs script, so a server-supplied
 * value is never passed through unchecked: only absolute https URLs survive.
 * Returns the normalised URL, or null when it must not be followed.
 */
export function safeCheckoutUrl(raw: string | null | undefined): string | null {
  if (!raw) return null;
  try {
    const url = new URL(raw);
    return url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null; // relative path or malformed
  }
}
