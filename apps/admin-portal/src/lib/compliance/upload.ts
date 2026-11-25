/**
 * Pure helpers behind the "Upload document" modal.
 *
 * Kept out of the component so the two rules that matter — the size guard and
 * the base64 prefix strip — can be tested without a DOM.
 */

/** The server rejects anything larger; failing here saves the round trip. */
export const MAX_UPLOAD_BYTES = 10 * 1024 * 1024;

export const ACCEPTED_CONTENT_TYPES = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "application/pdf",
] as const;

export const MAX_DOCUMENT_NUMBER_LENGTH = 100;

/**
 * `FileReader.readAsDataURL` yields `data:<mime>;base64,<payload>`. The API
 * wants the payload alone.
 */
export function stripDataUrlPrefix(dataUrl: string): string {
  const marker = ";base64,";
  const i = dataUrl.indexOf(marker);
  return i === -1 ? dataUrl : dataUrl.slice(i + marker.length);
}

/** `null` when the file is acceptable, otherwise a message fit to show the user. */
export function validateUploadFile(file: { size: number; type: string }): string | null {
  if (file.size === 0) return "The selected file is empty.";
  if (file.size > MAX_UPLOAD_BYTES) {
    const mb = (file.size / (1024 * 1024)).toFixed(1);
    return `File is too large (${mb} MB). The limit is 10 MB.`;
  }
  if (!(ACCEPTED_CONTENT_TYPES as readonly string[]).includes(file.type)) {
    return "Unsupported file type. Use a JPEG, PNG, WebP or PDF.";
  }
  return null;
}

/** Read a file as bare base64 (no `data:` prefix). */
export function readFileAsBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Could not read the selected file."));
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        reject(new Error("Could not read the selected file."));
        return;
      }
      resolve(stripDataUrlPrefix(reader.result));
    };
    reader.readAsDataURL(file);
  });
}
