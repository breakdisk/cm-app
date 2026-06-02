/**
 * KYC image processing pipeline.
 *
 * Step 1 (caller responsibility): grayscale via GrayscaleProcessor.tsx
 *   → outputs a temp JPEG ≤ 1280px
 * Step 2 (this module): resize (if somehow still oversized) + encode to WebP 75%
 *   → outputs { base64, contentType } ready for the compliance API
 *
 * WebP falls back to JPEG on devices that do not support WebP encoding
 * (iOS < 14 running old WebKit, very rare in practice for SDK 54 targets).
 */
import * as ImageManipulator from 'expo-image-manipulator';
import { Image } from 'react-native';

const MAX_DIM = 1280;

function getImageDimensions(uri: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) =>
    Image.getSize(uri, (width, height) => resolve({ width, height }), reject),
  );
}

export type KycContentType = 'image/webp' | 'image/jpeg';

export interface ProcessedImage {
  base64: string;
  contentType: KycContentType;
}

/**
 * Resize the image so neither dimension exceeds 1280 px, then compress to
 * WebP at 75 % quality (JPEG fallback). Returns base64-encoded bytes ready
 * to post to POST /api/v1/compliance/me/documents/upload.
 */
export async function processKycImage(uri: string): Promise<ProcessedImage> {
  const { width, height } = await getImageDimensions(uri);

  const actions: ImageManipulator.Action[] = [];
  if (width > MAX_DIM || height > MAX_DIM) {
    actions.push(
      width >= height
        ? { resize: { width: MAX_DIM } }
        : { resize: { height: MAX_DIM } },
    );
  }

  const sharedOpts = { compress: 0.75, base64: true } as const;

  try {
    const r = await ImageManipulator.manipulateAsync(uri, actions, {
      ...sharedOpts,
      format: ImageManipulator.SaveFormat.WEBP,
    });
    return { base64: r.base64!, contentType: 'image/webp' };
  } catch {
    // WebP encoder not available on this device / OS version → JPEG
    const r = await ImageManipulator.manipulateAsync(uri, actions, {
      ...sharedOpts,
      format: ImageManipulator.SaveFormat.JPEG,
    });
    return { base64: r.base64!, contentType: 'image/jpeg' };
  }
}
