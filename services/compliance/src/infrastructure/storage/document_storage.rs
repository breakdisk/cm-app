use anyhow::{bail, Context};

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const PRESIGN_TTL_SECS: u64 = 900;              // 15 minutes

pub struct DocumentStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl DocumentStorage {
    pub async fn new(cfg: &crate::config::StorageConfig) -> anyhow::Result<Self> {
        let aws_cfg = aws_config::from_env()
            .endpoint_url(&cfg.endpoint)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                &cfg.access_key, &cfg.secret_key, None, None, "static",
            ))
            .load()
            .await;
        let client = aws_sdk_s3::Client::new(&aws_cfg);
        Ok(Self { client, bucket: cfg.bucket.clone() })
    }

    /// Upload raw bytes; returns an `s3://bucket/key` URI stored in `driver_documents.file_url`.
    pub async fn upload(
        &self,
        tenant_id: uuid::Uuid,
        file_bytes: Vec<u8>,
        content_type: &str,
    ) -> anyhow::Result<String> {
        if file_bytes.len() > MAX_FILE_BYTES {
            bail!("File exceeds 10 MB limit");
        }
        if !matches!(content_type, "image/jpeg" | "image/png" | "application/pdf") {
            bail!("Invalid content type: must be image/jpeg, image/png, or application/pdf");
        }
        let key = format!("compliance/{}/{}", tenant_id, uuid::Uuid::new_v4());
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(aws_sdk_s3::primitives::ByteStream::from(file_bytes))
            .content_type(content_type)
            .send()
            .await
            .context("S3 upload failed")?;
        Ok(format!("s3://{}/{}", self.bucket, key))
    }

    /// Generate a 15-minute presigned GET URL for a stored document.
    pub async fn presign_url(&self, s3_uri: &str) -> anyhow::Result<String> {
        let key = s3_uri
            .strip_prefix(&format!("s3://{}/", self.bucket))
            .context("Invalid s3:// URI format")?;
        let presigned = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(PRESIGN_TTL_SECS),
                )?,
            )
            .await
            .context("Presign failed")?;
        Ok(presigned.uri().to_string())
    }
}
