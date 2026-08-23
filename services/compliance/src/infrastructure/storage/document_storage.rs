use anyhow::{bail, Context};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;  // 10 MB
pub(crate) const PRESIGN_TTL_SECS: u64 = 900;   // 15 minutes — used by handlers for `expires_in`

pub struct DocumentStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
    /// Set once the bucket has been confirmed to exist (HEAD ok or created).
    /// Lets the happy-path upload skip the defensive clone-and-retry below.
    bucket_ready: AtomicBool,
}

impl DocumentStorage {
    pub async fn new(cfg: &crate::config::StorageConfig) -> anyhow::Result<Self> {
        // Detect Cloudflare R2 by endpoint host.  R2 credentials are *exactly*
        // 32 chars; AWS S3 keys are 20 chars; MinIO accepts anything.  We've
        // seen deploys burn because the AWS SDK silently picked up a leftover
        // 20-char AWS_ACCESS_KEY_ID from the container env and R2 rejected
        // every PUT with "Credential access key has length 20, should be 32".
        // Panic at boot rather than at the first customer KYC upload.
        let is_r2 = cfg.endpoint.contains("r2.cloudflarestorage.com");

        if is_r2 {
            if cfg.access_key.len() != 32 {
                bail!(
                    "STORAGE__ACCESS_KEY is {} chars but Cloudflare R2 requires exactly 32. \
                     This is usually an AWS S3 key (20 chars) set instead of an R2 API token. \
                     Generate an R2 token at https://dash.cloudflare.com/?to=/:account/r2/api-tokens \
                     and update STORAGE__ACCESS_KEY / STORAGE__SECRET_KEY.",
                    cfg.access_key.len()
                );
            }
            // Explicit credentials_provider below already overrides any ambient
            // AWS_* env vars for this client — no need to scrub the process env
            // (which would be unsafe on a multi-threaded Tokio runtime).
            tracing::info!(key_len = 32, "compliance storage: targeting Cloudflare R2");
        }

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

        // force_path_style:
        //   - true  (path-style)    → required for MinIO (`bucket.minio:9000` fails DNS)
        //   - false (virtual-hosted) → required for Cloudflare R2 presigned PUTs
        //     (R2 rejects presigned PUTs generated with path-style addressing)
        //
        // Default: false when R2 is detected, true otherwise (MinIO default).
        // Override via STORAGE__FORCE_PATH_STYLE if needed.
        let force_path_style = cfg.force_path_style.unwrap_or(!is_r2);

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
        //
        // Skipped for R2: R2 buckets must be pre-provisioned in the dashboard;
        // the R2 API token typically lacks CreateBucket permission by design.
        if !is_r2 {
            let ready = storage.ensure_bucket().await;
            storage.bucket_ready.store(ready, Ordering::Relaxed);
        } else {
            storage.bucket_ready.store(true, Ordering::Relaxed);
        }

        Ok(storage)
    }

    /// Bucket name — used by handlers to reconstruct `s3://bucket/key` URIs
    /// for confirmed presigned uploads.
    pub fn bucket(&self) -> &str {
        &self.bucket
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

    /// Generate a 15-minute presigned PUT URL for direct-to-R2 upload from the
    /// customer app.  Returns `(upload_url, s3_key, upload_headers)` where
    /// `s3_key` is stored in `driver_documents.file_url` (after the caller
    /// wraps it in `s3://bucket/key`) and `upload_headers` are any headers the
    /// SDK says the client MUST include in the PUT request alongside the URL.
    ///
    /// Do NOT add `x-amz-content-sha256` as a sidecar header manually — the
    /// presigned URL already encodes UNSIGNED-PAYLOAD in the canonical request.
    /// Sending it as an unsigned header causes R2 to include it in its canonical
    /// request while our signature excludes it → "SignatureDoesNotMatch".
    pub async fn presign_upload(
        &self,
        tenant_id: uuid::Uuid,
        content_type: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<(String, String, std::collections::HashMap<String, String>)> {
        if !matches!(content_type, "image/jpeg" | "image/png" | "image/webp" | "application/pdf") {
            bail!("Invalid content type: must be image/jpeg, image/png, image/webp, or application/pdf");
        }
        let key = format!("compliance/{}/{}", tenant_id, uuid::Uuid::new_v4());

        let presigned = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(ttl_secs),
                )?,
            )
            .await
            .context("Failed to generate presigned upload URL")?;

        let url = presigned.uri().to_string();
        let headers: std::collections::HashMap<String, String> = presigned
            .headers()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

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
            "Generated compliance presigned PUT URL",
        );

        Ok((url, key, headers))
    }

    /// Upload raw bytes server-side; returns an `s3://bucket/key` URI stored in
    /// `driver_documents.file_url`.  Kept for backwards-compatibility (admin bulk
    /// ingest, driver-app callers).  New customer-app KYC uploads use
    /// `presign_upload` + `confirm_document` for a direct-to-R2 flow.
    pub async fn upload(
        &self,
        tenant_id: uuid::Uuid,
        file_bytes: Vec<u8>,
        content_type: &str,
    ) -> anyhow::Result<String> {
        if file_bytes.len() > MAX_FILE_BYTES {
            bail!("File exceeds 10 MB limit");
        }
        if !matches!(content_type, "image/jpeg" | "image/png" | "image/webp" | "application/pdf") {
            bail!("Invalid content type: must be image/jpeg, image/png, image/webp, or application/pdf");
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
    //
    // The error is boxed. `SdkError<PutObjectError>` is 368 bytes, which a
    // stable clippy newer than this repo's last green run rejects outright
    // (`clippy::result_large_err`) — every caller of a `Result` pays that size
    // on the success path too. Boxing is invisible here: both call sites end in
    // `.context(...)?`, and `Box<E>` implements `Error` whenever `E` does.
    ) -> Result<(), Box<aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>>>
    {
        self.client.put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(aws_sdk_s3::primitives::ByteStream::from(file_bytes))
            .content_type(content_type)
            .send()
            .await
            .map(|_| ())
            .map_err(Box::new)
    }

    /// Confirm that a key exists in the bucket (HeadObject).  Used by
    /// `confirm_document` to verify the caller actually uploaded a file before
    /// the document record is created in the database.
    pub async fn head_object(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .context("Object not found in storage — upload may have failed or the presigned URL expired")?;
        Ok(())
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
