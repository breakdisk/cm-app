use anyhow::{bail, Context};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const PRESIGN_TTL_SECS: u64 = 900;              // 15 minutes

pub struct DocumentStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
    /// Set once the bucket has been confirmed to exist (HEAD ok or created).
    /// Lets the happy-path upload skip the defensive clone-and-retry below.
    bucket_ready: AtomicBool,
}

impl DocumentStorage {
    pub async fn new(cfg: &crate::config::StorageConfig) -> anyhow::Result<Self> {
        // Always inject the region explicitly so the AWS SDK never falls back to
        // IMDS. On a non-AWS VPS, IMDS times out (1 s per call) and then the SDK
        // errors every S3 call with "A region must be set". Cloudflare R2 uses the
        // pseudo-region "auto"; MinIO accepts any non-empty value.
        let region_str = cfg.region.clone()
            .or_else(|| std::env::var("S3_REGION").ok())
            .or_else(|| std::env::var("AWS_REGION").ok())
            .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
            .unwrap_or_else(|| "auto".to_string());
        let region = aws_sdk_s3::config::Region::new(region_str);

        let sdk_cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region)
            .load()
            .await;

        // force_path_style is MANDATORY for MinIO: without it the SDK builds
        // virtual-hosted URLs (`bucket.minio:9000`) which fail DNS resolution
        // inside the Docker network. R2 works with either for direct API calls.
        let force_path_style = cfg.force_path_style.unwrap_or(true);

        let s3_cfg = aws_sdk_s3::config::Builder::from(&sdk_cfg)
            .endpoint_url(&cfg.endpoint)
            .force_path_style(force_path_style)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                &cfg.access_key, &cfg.secret_key, None, None, "static",
            ))
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_cfg);
        let storage = Self {
            client,
            bucket: cfg.bucket.clone(),
            bucket_ready: AtomicBool::new(false),
        };

        // MinIO does not auto-create buckets on first put_object — it returns
        // NoSuchBucket. There is no init container provisioning it, so we
        // self-heal here. Idempotent: existing bucket → no-op. If this fails
        // (e.g. MinIO not ready yet at startup — compliance only depends on it
        // with `service_started`), `upload()` retries the provisioning lazily
        // so a slow-starting backend never wedges KYC uploads permanently.
        let ready = storage.ensure_bucket().await;
        storage.bucket_ready.store(ready, Ordering::Relaxed);

        Ok(storage)
    }

    /// Ensure the configured bucket exists. Best-effort and non-fatal: on real
    /// AWS/R2 the bucket is pre-provisioned and the key may lack CreateBucket
    /// permission, so a failure is logged (uploads will then surface the real
    /// error) rather than blocking service startup. Returns `true` when the
    /// bucket is confirmed reachable/created.
    async fn ensure_bucket(&self) -> bool {
        if self.client.head_bucket().bucket(&self.bucket).send().await.is_ok() {
            return true; // already exists and we can reach it
        }
        match self.client.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => {
                tracing::info!(bucket = %self.bucket, "created compliance storage bucket");
                true
            }
            Err(e) => {
                let detail = format!("{:?}", e);
                if detail.contains("BucketAlreadyOwnedByYou") || detail.contains("BucketAlreadyExists") {
                    tracing::debug!(bucket = %self.bucket, "storage bucket already exists");
                    true
                } else {
                    tracing::warn!(
                        bucket = %self.bucket,
                        error = %detail,
                        "could not ensure storage bucket exists; will retry lazily on next upload",
                    );
                    false
                }
            }
        }
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

        // Fast path: the bucket was confirmed at startup, so move the bytes
        // straight into the request without keeping a copy for a retry.
        if self.bucket_ready.load(Ordering::Relaxed) {
            self.put_object(&key, file_bytes, content_type)
                .await
                .context("S3 upload failed")?;
            return Ok(format!("s3://{}/{}", self.bucket, key));
        }

        // Unconfirmed bucket (startup provisioning failed — e.g. MinIO was not
        // ready yet). Keep a copy so we can create the bucket and retry once on
        // the NoSuchBucket error instead of surfacing a permanent failure.
        match self.put_object(&key, file_bytes.clone(), content_type).await {
            Ok(()) => {
                self.bucket_ready.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                let detail = format!("{e:?}");
                let missing_bucket = detail.contains("NoSuchBucket") || detail.contains("NotFound");
                if !missing_bucket {
                    return Err(anyhow::Error::new(e).context("S3 upload failed"));
                }
                tracing::warn!(
                    bucket = %self.bucket,
                    "storage bucket missing on upload; provisioning and retrying once",
                );
                if self.ensure_bucket().await {
                    self.bucket_ready.store(true, Ordering::Relaxed);
                }
                self.put_object(&key, file_bytes, content_type)
                    .await
                    .context("S3 upload failed")?;
            }
        }
        Ok(format!("s3://{}/{}", self.bucket, key))
    }

    /// Single `put_object` call. Split out so `upload()` can issue it twice
    /// (initial attempt + post-provisioning retry) without duplicating the
    /// request builder.
    async fn put_object(
        &self,
        key: &str,
        file_bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>> {
        self.client.put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(aws_sdk_s3::primitives::ByteStream::from(file_bytes))
            .content_type(content_type)
            .send()
            .await
            .map(|_| ())
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
