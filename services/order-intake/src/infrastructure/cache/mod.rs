// Redis cache client — used for idempotency keys and rate-limit counters.
// Initialized in bootstrap and passed to handlers that need it.

use redis::aio::MultiplexedConnection;

pub struct RedisCache {
    conn: MultiplexedConnection,
}

impl RedisCache {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    /// Check and set an idempotency key (TTL = 24 hours).
    /// Returns true if the key was newly set (first time), false if it already existed.
    pub async fn set_idempotency_key(&mut self, key: &str) -> anyhow::Result<bool> {
        use redis::AsyncCommands;
        let set: bool = self.conn.set_nx(key, 1u8).await?;
        if set {
            let _: () = self.conn.expire(key, 86400).await?;
        }
        Ok(set)
    }
}
