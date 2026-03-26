// External HTTP client — business-logic service
// Used by RuleActions that call external webhooks or other LogisticOS services
// (e.g., trigger_reassign → POST dispatch/v1/assignments).

use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct ExternalHttpClient {
    client: Client,
}

impl ExternalHttpClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self { client }
    }

    /// POST JSON body to an internal service URL.
    pub async fn post_json(
        &self,
        url: &str,
        body: &Value,
    ) -> Result<Value, reqwest::Error> {
        let resp = self.client.post(url).json(body).send().await?;
        let json = resp.json::<Value>().await?;
        Ok(json)
    }

    /// POST to a merchant-configured webhook endpoint.
    pub async fn post_webhook(
        &self,
        url: &str,
        payload: &Value,
        secret: Option<&str>,
    ) -> Result<u16, reqwest::Error> {
        let mut req = self.client.post(url).json(payload);
        if let Some(s) = secret {
            req = req.header("X-Webhook-Secret", s);
        }
        let resp = req.send().await?;
        Ok(resp.status().as_u16())
    }
}

impl Default for ExternalHttpClient {
    fn default() -> Self {
        Self::new()
    }
}
