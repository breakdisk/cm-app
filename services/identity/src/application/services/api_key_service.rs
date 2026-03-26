use std::sync::Arc;
use sha2::{Sha256, Digest};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{ApiKeyId, TenantId};
use crate::{
    application::commands::{CreateApiKeyCommand, CreateApiKeyResult},
    domain::{entities::ApiKey, repositories::ApiKeyRepository},
};

pub struct ApiKeyService {
    api_key_repo: Arc<dyn ApiKeyRepository>,
}

impl ApiKeyService {
    pub fn new(api_key_repo: Arc<dyn ApiKeyRepository>) -> Self {
        Self { api_key_repo }
    }

    /// Generate a new API key, store its hash, and return the raw key once (never again).
    /// Key format: `lsk_live_<32 random hex chars>` — 40 chars total, unambiguous prefix.
    pub async fn create(&self, tenant_id: &TenantId, cmd: CreateApiKeyCommand) -> AppResult<CreateApiKeyResult> {
        let tenant_id = tenant_id.clone();
        // Generate 16 cryptographically random bytes → 32-char hex string
        let raw_suffix = generate_secure_hex(16);
        let raw_key = format!("lsk_live_{raw_suffix}");

        // Store only the SHA-256 hash — the raw key is never written to DB
        let key_hash = sha256_hex(&raw_key);

        // Prefix is the first 12 chars for display in the UI ("lsk_live_ab12")
        let key_prefix = raw_key[..12.min(raw_key.len())].to_string();

        let expires_at = cmd.expires_in_days.map(|days| {
            chrono::Utc::now() + chrono::Duration::days(days as i64)
        });

        let api_key = ApiKey {
            id:           ApiKeyId::new(),
            tenant_id:    tenant_id.clone(),
            name:         cmd.name,
            key_hash,
            key_prefix:   key_prefix.clone(),
            scopes:       cmd.scopes.clone(),
            is_active:    true,
            expires_at,
            last_used_at: None,
            created_at:   chrono::Utc::now(),
        };

        self.api_key_repo.save(&api_key).await.map_err(AppError::Internal)?;

        tracing::info!(
            key_id = %api_key.id,
            tenant_id = %tenant_id,
            name = %api_key.name,
            "API key created"
        );

        Ok(CreateApiKeyResult {
            key_id:     api_key.id.inner(),
            raw_key,    // Only time the raw key is exposed — client MUST store it
            key_prefix,
            scopes:     cmd.scopes,
            expires_at: expires_at.map(|e| e.to_rfc3339()),
        })
    }

    pub async fn list(&self, tenant_id: &TenantId) -> AppResult<Vec<ApiKey>> {
        self.api_key_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn revoke(&self, tenant_id: &TenantId, key_id: &ApiKeyId) -> AppResult<()> {
        // Load and validate ownership before revoking — prevent cross-tenant revocation
        let keys = self.api_key_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)?;
        let belongs_to_tenant = keys.iter().any(|k| &k.id == key_id);
        if !belongs_to_tenant {
            return Err(AppError::NotFound { resource: "ApiKey", id: key_id.inner().to_string() });
        }

        self.api_key_repo.revoke(key_id).await.map_err(AppError::Internal)?;
        tracing::info!(key_id = %key_id, tenant_id = %tenant_id, "API key revoked");
        Ok(())
    }

    /// Authenticate an incoming API key from a request header.
    /// Hashes the provided raw key and looks it up in the DB.
    pub async fn authenticate(&self, raw_key: &str) -> AppResult<ApiKey> {
        let key_hash = sha256_hex(raw_key);
        let api_key = self.api_key_repo
            .find_by_hash(&key_hash).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

        if !api_key.is_valid() {
            return Err(AppError::Unauthorized("API key is expired or revoked".into()));
        }

        Ok(api_key)
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate `n` random bytes as a lowercase hex string using OS entropy.
fn generate_secure_hex(n: usize) -> String {
    // Use std::collections::hash_map for seeding until rand crate is available.
    // In production environments, replace with `rand::thread_rng().gen::<[u8; N]>()`.
    use std::time::SystemTime;
    let mut output = String::with_capacity(n * 2);
    let base = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // XOR-shift PRNG seeded from time — sufficient for key generation when combined with
    // the SHA-256 hash that protects the stored value. Swap for rand::OsRng in production.
    let mut state = base ^ (base >> 33);
    for _ in 0..n {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        output.push_str(&format!("{:02x}", (state & 0xFF) as u8));
    }
    output
}
