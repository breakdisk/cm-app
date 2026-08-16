//! S3-compatible object storage adapter for carrier label files.
//! Mirrors the pattern in services/pod/src/infrastructure/external/storage.rs
//! with an additional `put_bytes` method for server-side label upload.

use async_trait::async_trait;

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Server-side PUT — upload bytes directly (used for DHL inline label PDF).
    async fn put_bytes(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> anyhow::Result<()>;

    /// Generate a pre-signed GET URL for downloading a label (merchant portal / API response).
    async fn presign_download(&self, key: &str, ttl_seconds: u32) -> anyhow::Result<String>;
}

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
        credentials: Option<(String, String)>,
    ) -> anyhow::Result<Self> {
        let region_str = region.unwrap_or_else(|| "auto".to_string());
        let aws_region = aws_sdk_s3::config::Region::new(region_str);

        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_region)
            .load()
            .await;

        let mut builder = aws_sdk_s3::config::Builder::from(&sdk_config);

        if let Some((access_key_id, secret_access_key)) = credentials {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("AWS_SESSION_TOKEN");
            let creds = aws_sdk_s3::config::Credentials::new(
                access_key_id,
                secret_access_key,
                None,
                None,
                "logisticos-static",
            );
            builder = builder.credentials_provider(creds);
        }

        if let Some(endpoint) = endpoint_url {
            builder = builder.endpoint_url(endpoint).force_path_style(force_path_style);
        }

        let client = aws_sdk_s3::Client::from_conf(builder.build());
        Ok(Self { client, bucket })
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn put_bytes(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes))
            .send()
            .await?;
        tracing::info!(bucket = %self.bucket, key = %key, "Uploaded label to R2/S3");
        Ok(())
    }

    async fn presign_download(&self, key: &str, ttl_seconds: u32) -> anyhow::Result<String> {
        let presigned = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(ttl_seconds as u64),
                )?,
            )
            .await?;
        Ok(presigned.uri().to_string())
    }
}
