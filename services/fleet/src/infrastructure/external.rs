use uuid::Uuid;
use logisticos_errors::AppResult;

pub struct FleetExternalClient {
    http_client: reqwest::Client,
    fleet_service_url: String,
}

impl FleetExternalClient {
    pub fn new(http_client: reqwest::Client, fleet_service_url: String) -> Self {
        Self {
            http_client,
            fleet_service_url,
        }
    }

    /// Check if a vehicle is operational (for marketplace dead-man's switch)
    pub async fn is_vehicle_operational(&self, vehicle_id: Uuid) -> AppResult<bool> {
        let url = format!("{}/v1/internal/vehicles/{}/operational", self.fleet_service_url, vehicle_id);
        let response = self.http_client.get(&url).send().await?;
        if response.status() == 200 {
            let body: serde_json::Value = response.json().await?;
            Ok(body.get("data")
                .and_then(|v| v.get("is_operational"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false))
        } else {
            Ok(false)
        }
    }
}
