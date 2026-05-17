use redis::aio::Connection;
use uuid::Uuid;

pub struct FleetCache {
    redis: Connection,
}

impl FleetCache {
    pub fn new(redis: Connection) -> Self {
        Self { redis }
    }

    pub async fn get_vehicle_status(&self, vehicle_id: Uuid) -> redis::RedisResult<Option<String>> {
        redis::cmd("GET")
            .arg(format!("vehicle:{}:status", vehicle_id))
            .query_async(&mut self.redis.clone())
            .await
    }

    pub async fn set_vehicle_status(&mut self, vehicle_id: Uuid, status: &str, ttl_secs: usize) -> redis::RedisResult<()> {
        redis::cmd("SETEX")
            .arg(format!("vehicle:{}:status", vehicle_id))
            .arg(ttl_secs)
            .arg(status)
            .query_async(&mut self.redis)
            .await
    }
}
