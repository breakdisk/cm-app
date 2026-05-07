//! S3-compatible object storage adapter for POD photos.
//! Uses the AWS SDK for Rust (aws-sdk-s3) to generate pre-signed PUT URLs.
//! Compatible with MinIO in local dev and AWS S3 in production.

use async_trait::async_trait;

/// Result of generating a pre-signed upload URL.
/// `headers` contains any headers the client MUST include in its PUT request alongside
/// the presigned URL — typically `x-amz-content-sha256: UNSIGNED-PAYLOAD` and
/// `x-amz-date: <timestamp>` when Cloudflare R2 includes them in SignedHeaders.
pub struct PresignedUpload {
    pub url: String,
    pub headers: std::collections::HashMap<String, String>,
}

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Generate a pre-signed URL for direct-upload from the driver app.
    /// `ttl_seconds` is how long the URL is valid.
    async fn presign_upload(&self, key: &str, content_type: &str, ttl_seconds: u32) -> anyhow::Result<PresignedUpload>;

    /// Generate a pre-signed URL for viewing a photo (for merchant portal).
    async fn presign_download(&self, key: &str, ttl_seconds: u32) -> anyhow::Result<String>;

    /// Delete a photo (for disputed/cancelled PODs).
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
}

/// S3/MinIO implementation using the AWS SDK.
pub struct S3StorageAdapter {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3StorageAdapter {
    pub async fn new(
        endpoint_url: Option<String>,
        bucket: String,
        region: Option<String>,
        force_path_style: bool,
    ) -> anyhow::Result<Self> {
        // The AWS SDK auto-discovers region from env/IMDS. On a non-AWS host
        // (our VPS), IMDS times out (1 s per call) and the SDK then errors
        // every S3 call with "A region must be set". We always provide an
        // explicit region — `auto` is what Cloudflare R2 expects, and any
        // value is accepted by MinIO / non-AWS endpoints.
        let region_str = region.unwrap_or_else(|| "auto".to_string());
        let aws_region = aws_sdk_s3::config::Region::new(region_str);

        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_region)
            .load()
            .await;

        // Build the SDK config — endpoint_url overrides for MinIO/R2 in local/staging.
        // force_path_style: true for MinIO (requires it), false for Cloudflare R2
        // (R2 rejects presigned PUTs generated with path-style).
        let s3_config = if let Some(endpoint) = endpoint_url {
            aws_sdk_s3::config::Builder::from(&sdk_config)
                .endpoint_url(endpoint)
                .force_path_style(force_path_style)
                .build()
        } else {
            aws_sdk_s3::config::Builder::from(&sdk_config).build()
        };

        let client = aws_sdk_s3::Client::from_conf(s3_config);
        Ok(Self { client, bucket })
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn presign_upload(&self, key: &str, content_type: &str, ttl_seconds: u32) -> anyhow::Result<PresignedUpload> {
        let presigned = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(ttl_seconds as u64)
                )?
            )
            .await?;

        // Collect all headers the SDK says the client MUST send with the PUT request.
        // For Cloudflare R2, this includes `x-amz-content-sha256: UNSIGNED-PAYLOAD`
        // and `x-amz-date: <timestamp>` when both appear in X-Amz-SignedHeaders.
        // Returning them here means the Android client sends exactly what R2 signed —
        // no hard-coding of header names or values in the client.
        let headers: std::collections::HashMap<String, String> = presigned
            .headers()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Ok(PresignedUpload { url: presigned.uri().to_string(), headers })
    }

    async fn presign_download(&self, key: &str, ttl_seconds: u32) -> anyhow::Result<String> {
        let presigner = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(ttl_seconds as u64)
                )?
            )
            .await?;
        Ok(presigner.uri().to_string())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        Ok(())
    }
}
