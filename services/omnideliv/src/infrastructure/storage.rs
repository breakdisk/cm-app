//! Product photo storage.
//!
//! Deliberately not the presigned-URL pattern the compliance service uses. The
//! bucket here is `minio` on the internal compose network — no published port,
//! no Traefik route — so a presigned URL handed to a browser points somewhere
//! the browser cannot reach. Bytes go through the service in both directions,
//! which is affordable for catalog photos and is the only thing that works
//! against a cluster-internal store.
//!
//! Configured absent, storage is `None` rather than a boot failure: OmniDeliv
//! ran without photos before this existed and an environment that has not set
//! the vars yet should keep serving catalogs, with the upload route reporting
//! that it is unconfigured.

use anyhow::{bail, Context};

/// Cap on a single upload. A catalog photo that needs more than this is a
/// mis-selected original, not a product shot.
pub const MAX_PHOTO_BYTES: usize = 5 * 1024 * 1024;

/// What a client may send, for the formats a plain prefix identifies. Checked
/// against the *sniffed* bytes, not the caller's Content-Type, which is a claim.
const ALLOWED_PREFIX: &[(&str, &[u8])] = &[
    ("image/jpeg", &[0xFF, 0xD8, 0xFF]),
    ("image/png", &[0x89, b'P', b'N', b'G']),
];

/// Content type implied by the leading bytes, or `None` if it is not an image
/// we accept.
///
/// WebP is deliberately not in the prefix table. It is a RIFF container — so
/// are WAV and AVI — and the format is named by the four bytes at offset 8, not
/// at 0. Matching `RIFF` alone accepted a WAV as `image/webp` and stored it
/// under that content type, which is then what it is served back with.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    ALLOWED_PREFIX
        .iter()
        .find(|(_, magic)| bytes.starts_with(magic))
        .map(|(ct, _)| *ct)
}

pub struct PhotoStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl PhotoStorage {
    pub async fn new(cfg: &crate::config::StorageConfig) -> anyhow::Result<Self> {
        if cfg.endpoint.trim().is_empty() || cfg.bucket.trim().is_empty() {
            bail!("storage endpoint and bucket must both be set");
        }

        // Always explicit, never inferred. On a non-AWS VPS the SDK otherwise
        // falls back to IMDS, which times out once per call and then fails
        // every request with "A region must be set".
        let region = aws_sdk_s3::config::Region::new(
            cfg.region.clone().unwrap_or_else(|| "us-east-1".to_string()),
        );

        let sdk_cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region)
            .load()
            .await;

        let s3_cfg = aws_sdk_s3::config::Builder::from(&sdk_cfg)
            .endpoint_url(cfg.endpoint.clone())
            // Path style is required for MinIO: virtual-hosted addressing
            // resolves `bucket.minio:9000`, which has no DNS entry.
            .force_path_style(cfg.force_path_style)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                cfg.access_key.clone(),
                cfg.secret_key.clone(),
                None,
                None,
                "omnideliv-storage",
            ))
            .build();

        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(s3_cfg),
            bucket: cfg.bucket.clone(),
        })
    }

    /// Create the bucket if it is not there yet.
    ///
    /// Called once at boot. A missing bucket is otherwise discovered by the
    /// first vendor who tries to upload, which makes a deployment problem look
    /// like a broken feature.
    pub async fn ensure_bucket(&self) -> anyhow::Result<()> {
        if self.client.head_bucket().bucket(&self.bucket).send().await.is_ok() {
            return Ok(());
        }
        self.client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .with_context(|| format!("creating bucket {}", self.bucket))?;
        Ok(())
    }

    pub async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(bytes.into())
            .send()
            .await
            .with_context(|| format!("uploading {key}"))?;
        Ok(())
    }

    /// Fetch an object. `Ok(None)` when the key is absent — a photo whose row
    /// survived its object is a 404, not a 500.
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<(Vec<u8>, String)>> {
        let out = match self.client.get_object().bucket(&self.bucket).key(key).send().await {
            Ok(o) => o,
            Err(e) => {
                let svc = e.into_service_error();
                if svc.is_no_such_key() {
                    return Ok(None);
                }
                return Err(anyhow::anyhow!(svc).context(format!("fetching {key}")));
            }
        };
        let content_type = out.content_type().unwrap_or("application/octet-stream").to_string();
        let bytes = out.body.collect().await.context("reading object body")?.into_bytes().to_vec();
        Ok(Some((bytes.to_vec(), content_type)))
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("deleting {key}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Content-Type a browser sends is a claim by the caller. These are the
    /// bytes, which are not.
    #[test]
    fn sniffs_the_formats_we_accept() {
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(sniff_image(b"RIFF____WEBP"), Some("image/webp"));
    }

    /// A minimal RIFF header: "RIFF", little-endian payload length, then the
    /// four-byte form type that actually names the format.
    fn riff(form_type: &[u8; 4], payload_len: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&payload_len.to_le_bytes());
        v.extend_from_slice(form_type);
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    /// RIFF is a container, not a format — WAV and AVI use it too. Matching
    /// `RIFF` alone stored a WAV as `image/webp`, and that is then the content
    /// type it is served back under. Proof photos are about to depend on this.
    #[test]
    fn a_riff_container_that_is_not_webp_is_refused() {
        assert_eq!(sniff_image(&riff(b"WEBP", 32)), Some("image/webp"));
        assert_eq!(sniff_image(&riff(b"WAVE", 32)), None);
        assert_eq!(sniff_image(&riff(b"AVI ", 32)), None);
    }

    /// Four bytes of "RIFF" and nothing else must not index past the end.
    #[test]
    fn a_truncated_riff_header_does_not_panic() {
        assert_eq!(sniff_image(b"RIFF"), None);
        // One byte short of the form type: the check must not read past
        // the end to find out.
        assert_eq!(sniff_image(&riff(b"WEBP", 32)[..11]), None);
    }

    #[test]
    fn refuses_anything_else() {
        // An SVG is an image to a human and a script host to a browser, which
        // is why it is not on the list.
        assert_eq!(sniff_image(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"), None);
        assert_eq!(sniff_image(b"GIF89a"), None);
        assert_eq!(sniff_image(b""), None);
        assert_eq!(sniff_image(b"\xFF\xD8"), None, "a truncated JPEG magic is not a JPEG");
    }
}
