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
    /// `credentials` — explicit (access_key_id, secret_access_key) for R2 or MinIO.
    /// When `None`, the AWS SDK auto-discovers from env/IMDS (fine for real AWS S3).
    /// Always pass explicit credentials for Cloudflare R2: R2 keys are 32 chars, but
    /// the SDK auto-discovery chain picks up any `AWS_ACCESS_KEY_ID` in the environment,
    /// which may be a legacy 20-char AWS key that R2 rejects with "InvalidArgument:
    /// Credential access key has length 20, should be 32".
    pub async fn new(
        endpoint_url: Option<String>,
        bucket: String,
        region: Option<String>,
        force_path_style: bool,
        credentials: Option<(String, String)>,
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
        let mut builder = aws_sdk_s3::config::Builder::from(&sdk_config);

        // Inject explicit credentials when provided. This overrides whatever
        // AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY the SDK would auto-discover,
        // ensuring the correct R2 key (32 chars) is embedded in presigned URLs.
        if let Some((access_key_id, secret_access_key)) = credentials {
            let creds = aws_sdk_s3::config::Credentials::new(
                access_key_id,
                secret_access_key,
                None,   // session token — not used for R2 or static MinIO keys
                None,   // expiry — static key, never expires
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
    async fn presign_upload(&self, key: &str, content_type: &str, ttl_seconds: u32) -> anyhow::Result<PresignedUpload> {
        // Sign Content-Type as part of the canonical request. The Android client
        // sends `Content-Type: image/jpeg` on the PUT body, and if Content-Type is
        // not in SignedHeaders R2's signature validator can take a different code
        // path that demands x-amz-date as a header (which is not how presigned
        // URLs work). Calling .content_type() here adds it to SignedHeaders so
        // the validator stays on the presigned-URL code path.
        let presigned = self.client
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

        let url = presigned.uri().to_string();

        // Headers the SDK says the client MUST send.
        let mut headers: std::collections::HashMap<String, String> = presigned
            .headers()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        // R2-specific quirk: R2 requires `x-amz-content-sha256` as an actual
        // request header on every PUT, even for presigned URLs where the SDK
        // doesn't put it in SignedHeaders. Sending it unsigned is fine for V4 —
        // unsigned headers are simply not part of the canonical request — but
        // R2's request gate rejects requests that omit it entirely with
        // "InvalidRequest: Missing x-amz-content-sha256".
        //
        // This is safe to add unconditionally: if the SDK already returned it,
        // .or_insert_with is a no-op; if not, we add UNSIGNED-PAYLOAD which
        // matches what aws-sdk-s3 uses for presigned URL body hashing.
        //
        // We deliberately do NOT inject x-amz-date as a header — sending it
        // makes R2 switch to direct-V4 validation looking for the Authorization
        // header, which doesn't exist on presigned requests.
        headers
            .entry("x-amz-content-sha256".to_string())
            .or_insert_with(|| "UNSIGNED-PAYLOAD".to_string());

        // Log the SignedHeaders list from the URL so a botched config (path-style,
        // wrong endpoint, missing region) is obvious from container logs.
        let signed_headers = url
            .split("X-Amz-SignedHeaders=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .unwrap_or("<none>");
        tracing::info!(
            bucket = %self.bucket,
            key = %key,
            content_type = %content_type,
            signed_headers = %signed_headers,
            header_count = headers.len(),
            "Generated R2 presigned PUT URL"
        );

        Ok(PresignedUpload { url, headers })
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
