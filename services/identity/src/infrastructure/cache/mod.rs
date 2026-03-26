//! Redis-backed cache for the identity service.
//! Tracks revoked refresh tokens and enforces login attempt rate limits.

use redis::AsyncCommands;
use anyhow::Result;

pub struct RedisCache {
    client: redis::Client,
}

impl RedisCache {
    pub fn new(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        Ok(Self { client })
    }

    async fn conn(&self) -> Result<redis::aio::MultiplexedConnection> {
        Ok(self.client.get_multiplexed_async_connection().await?)
    }

    /// Mark a refresh token JTI as revoked. TTL matches token expiry so the
    /// blocklist self-prunes without a background job.
    pub async fn revoke_refresh_token(&self, jti: &str, ttl_seconds: u64) -> Result<()> {
        let key = format!("identity:revoked_jti:{jti}");
        let mut conn = self.conn().await?;
        conn.set_ex::<_, _, ()>(&key, 1u8, ttl_seconds).await?;
        Ok(())
    }

    /// Returns true if this JTI has been explicitly revoked (logout or rotation).
    pub async fn is_refresh_token_revoked(&self, jti: &str) -> Result<bool> {
        let key = format!("identity:revoked_jti:{jti}");
        let mut conn = self.conn().await?;
        let exists: bool = conn.exists(&key).await?;
        Ok(exists)
    }

    /// Sliding-window login attempt counter per (tenant_slug, email).
    /// Returns the current attempt count after incrementing.
    pub async fn increment_login_attempts(&self, tenant_slug: &str, email: &str) -> Result<u32> {
        let key = format!("identity:login_attempts:{tenant_slug}:{email}");
        let mut conn = self.conn().await?;
        let count: u32 = conn.incr(&key, 1u32).await?;
        // Set 15-minute window on first attempt
        if count == 1 {
            conn.expire::<_, ()>(&key, 900).await?;
        }
        Ok(count)
    }

    /// Clear the attempt counter on successful login.
    pub async fn reset_login_attempts(&self, tenant_slug: &str, email: &str) -> Result<()> {
        let key = format!("identity:login_attempts:{tenant_slug}:{email}");
        let mut conn = self.conn().await?;
        conn.del::<_, ()>(&key).await?;
        Ok(())
    }

    /// Business rule: lock account after 5 failed attempts within the 15-minute window.
    pub async fn is_account_locked(&self, tenant_slug: &str, email: &str) -> Result<bool> {
        let key = format!("identity:login_attempts:{tenant_slug}:{email}");
        let mut conn = self.conn().await?;
        let count: Option<u32> = conn.get(&key).await?;
        Ok(count.unwrap_or(0) >= 5)
    }
}
