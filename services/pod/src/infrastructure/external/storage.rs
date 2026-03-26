//! S3-compatible object storage adapter for POD photos.
//! Uses the AWS SDK for Rust (aws-sdk-s3) to generate pre-signed PUT URLs.
//! Compatible with MinIO in local dev and AWS S3 in production.

use async_trait::async_trait;

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Generate a pre-signed URL for direct-upload from the driver app.
    /// `ttl_seconds` is how long the URL is valid.
    async fn presign_upload(&self, key: &str, content_type: &str, ttl_seconds: u32) -> anyhow::Result<String>;

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
    pub async fn new(endpoint_url: Option<String>, bucket: String) -> anyhow::Result<Self> {
        let mut config_builder = aws_config::load_from_env().await;
        // Build the SDK config — endpoint_url overrides for MinIO in local/staging
        let sdk_config = if let Some(endpoint) = endpoint_url {
            aws_sdk_s3::config::Builder::from(&config_builder)
                .endpoint_url(endpoint)
                .force_path_style(true)  // MinIO requires path-style
                .build()
        } else {
            aws_sdk_s3::config::Builder::from(&config_builder).build()
        };

        let client = aws_sdk_s3::Client::from_conf(sdk_config);
        Ok(Self { client, bucket })
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn presign_upload(&self, key: &str, content_type: &str, ttl_seconds: u32) -> anyhow::Result<String> {
        let presigner = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(ttl_seconds as u64)
                )?
            )
            .await?;
        Ok(presigner.uri().to_string())
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
