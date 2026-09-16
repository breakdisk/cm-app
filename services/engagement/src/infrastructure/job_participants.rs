//! Who is on this job, and how to reach them.
//!
//! engagement does not own those answers and must not invent them. order-intake
//! says whether a shipment is the caller's; driver-ops says whether this driver
//! is on it. Both are asked with the caller's own token, so the answer is
//! exactly what those services would give the caller directly — there is no
//! second, parallel authorization here to drift out of step with theirs.

use logisticos_errors::{AppError, AppResult};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::entities::job_message::SenderRole;

#[derive(Debug, Clone)]
pub struct Participation {
    pub role: SenderRole,
    /// False once the job is over: the thread stays readable and stops taking
    /// new messages.
    pub can_send: bool,
    pub shipment_status: Option<String>,
}

/// The two lines for a masked call. Neither is ever returned to an app — they
/// go from here straight to the call bridge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobContacts {
    /// The caller's own line: the leg the platform rings first.
    pub mine: Option<String>,
    /// The other party's line.
    pub theirs: Option<String>,
}

pub struct JobParticipants {
    order_intake_url: String,
    driver_ops_url: String,
    delivery_experience_url: String,
    client: reqwest::Client,
}

impl JobParticipants {
    pub fn new(order_intake_url: String, driver_ops_url: String, delivery_experience_url: String) -> Self {
        Self {
            order_intake_url,
            driver_ops_url,
            delivery_experience_url,
            client: reqwest::Client::new(),
        }
    }

    /// The caller's place on this job, or `Forbidden` when they have none.
    ///
    /// Operators are deliberately not included: a tenant-wide token can read
    /// the shipment, but this thread is between the two people doing the job.
    pub async fn resolve(
        &self,
        shipment_id: Uuid,
        user_id: Uuid,
        is_driver: bool,
        bearer: &str,
    ) -> AppResult<Participation> {
        // A driver's token is checked against driver-ops first — it would also
        // pass order-intake's tenant-wide read, and that is not what makes
        // someone a participant here.
        if is_driver {
            if let Some(found) = self.driver_on_job(shipment_id, bearer).await? {
                return Ok(found);
            }
        }
        if let Some(found) = self.customer_of_job(shipment_id, user_id, bearer).await? {
            return Ok(found);
        }
        if !is_driver {
            if let Some(found) = self.driver_on_job(shipment_id, bearer).await? {
                return Ok(found);
            }
        }
        Err(AppError::Forbidden { resource: "job chat".to_owned() })
    }

    /// Each side's number, from the service that holds it, with the caller's
    /// own token. A number that cannot be found comes back as `None` and the
    /// bridge declines rather than guessing.
    pub async fn contacts(&self, shipment_id: Uuid, role: SenderRole, bearer: &str) -> AppResult<JobContacts> {
        match role {
            SenderRole::Customer => {
                let shipment = self
                    .get_json(&format!("{}/v1/shipments/{shipment_id}", self.order_intake_url.trim_end_matches('/')), bearer)
                    .await
                    .unwrap_or(Value::Null);
                // The driver's line lives on the tracking record. It reaches
                // this bridge and nothing else — the customer app is never
                // given it.
                let tracking = self
                    .get_json(&format!("{}/v1/tracking/{shipment_id}", self.delivery_experience_url.trim_end_matches('/')), bearer)
                    .await
                    .unwrap_or(Value::Null);
                Ok(JobContacts {
                    mine: string_field(&shipment, "customer_phone"),
                    theirs: string_field(&tracking, "driver_phone"),
                })
            }
            SenderRole::Driver => {
                let me = self
                    .get_json(&format!("{}/v1/drivers/me", self.driver_ops_url.trim_end_matches('/')), bearer)
                    .await
                    .unwrap_or(Value::Null);
                let tasks = self
                    .get_json(&format!("{}/v1/tasks", self.driver_ops_url.trim_end_matches('/')), bearer)
                    .await
                    .unwrap_or(Value::Null);
                let theirs = tasks
                    .get("data")
                    .and_then(Value::as_array)
                    .and_then(|list| list.iter().find(|task| uuid_field(task, "shipment_id") == Some(shipment_id)))
                    .and_then(|task| string_field(task, "customer_phone"));
                Ok(JobContacts { mine: string_field(&me, "phone"), theirs })
            }
        }
    }

    async fn get_json(&self, url: &str, bearer: &str) -> AppResult<Value> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, bearer)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{url} unreachable: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::Internal(anyhow::anyhow!("{url} answered {}", response.status())));
        }
        response
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{url} sent no json: {e}")))
    }

    async fn customer_of_job(
        &self,
        shipment_id: Uuid,
        user_id: Uuid,
        bearer: &str,
    ) -> AppResult<Option<Participation>> {
        let url = format!("{}/v1/shipments/{}", self.order_intake_url.trim_end_matches('/'), shipment_id);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, bearer)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("order-intake unreachable: {e}")))?;

        let status = response.status();
        // Not theirs, or not there: not the customer of this job. Anything else
        // is a fault, and must not read as "no access".
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::FORBIDDEN {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AppError::Internal(anyhow::anyhow!("order-intake answered {status}")));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("order-intake sent no shipment: {e}")))?;
        let shipment = body.get("data").unwrap_or(&body);

        // The booker is stamped as the caller on create; `customer_id` is the
        // recipient. Either being this user makes them the customer side.
        let booked_by_them = uuid_field(shipment, "merchant_id") == Some(user_id);
        let named_as_them = uuid_field(shipment, "customer_id") == Some(user_id);
        if !booked_by_them && !named_as_them {
            return Ok(None);
        }

        let shipment_status = shipment.get("status").and_then(Value::as_str).map(str::to_owned);
        Ok(Some(Participation {
            role: SenderRole::Customer,
            can_send: shipment_status.as_deref().is_none_or(job_is_open),
            shipment_status,
        }))
    }

    /// driver-ops lists a driver's open tasks. A finished job drops off that
    /// list, so a driver's access to the thread ends with the job — which is
    /// also what the design promises about masked contact.
    async fn driver_on_job(&self, shipment_id: Uuid, bearer: &str) -> AppResult<Option<Participation>> {
        let url = format!("{}/v1/tasks", self.driver_ops_url.trim_end_matches('/'));
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, bearer)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("driver-ops unreachable: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::FORBIDDEN {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AppError::Internal(anyhow::anyhow!("driver-ops answered {status}")));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("driver-ops sent no tasks: {e}")))?;
        let tasks = body.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
        let on_this_job = tasks.iter().any(|task| uuid_field(task, "shipment_id") == Some(shipment_id));

        Ok(on_this_job.then_some(Participation {
            role: SenderRole::Driver,
            can_send: true,
            shipment_status: None,
        }))
    }
}

/// Ids cross service boundaries as a bare uuid string or as a wrapper object,
/// depending on the newtype. Take either rather than tying this to one shape.
fn uuid_field(value: &Value, key: &str) -> Option<Uuid> {
    let field = value.get(key)?;
    if let Some(text) = field.as_str() {
        return Uuid::parse_str(text).ok();
    }
    field
        .as_object()?
        .values()
        .find_map(|inner| inner.as_str().and_then(|text| Uuid::parse_str(text).ok()))
}

/// A string from a response that may or may not wrap its payload in `data`.
/// An empty string is nothing, not a value.
fn string_field(value: &Value, key: &str) -> Option<String> {
    let body = value.get("data").unwrap_or(value);
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A job that is finished keeps its thread readable and stops taking messages.
pub fn job_is_open(status: &str) -> bool {
    !matches!(
        status.to_ascii_lowercase().as_str(),
        "delivered" | "cancelled" | "canceled" | "returned" | "failed"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_id_is_read_as_a_string_or_as_a_wrapper() {
        let id = Uuid::new_v4();
        assert_eq!(uuid_field(&json!({ "merchant_id": id.to_string() }), "merchant_id"), Some(id));
        assert_eq!(uuid_field(&json!({ "merchant_id": { "0": id.to_string() } }), "merchant_id"), Some(id));
    }

    #[test]
    fn a_missing_or_unparseable_id_is_nobody() {
        let id = Uuid::new_v4();
        assert_eq!(uuid_field(&json!({ "customer_id": id.to_string() }), "merchant_id"), None);
        assert_eq!(uuid_field(&json!({ "merchant_id": "not-a-uuid" }), "merchant_id"), None);
        assert_eq!(uuid_field(&json!({ "merchant_id": 7 }), "merchant_id"), None);
    }

    #[test]
    fn a_number_is_read_through_a_data_wrapper_or_without_one() {
        assert_eq!(string_field(&json!({ "phone": "+639171234567" }), "phone").as_deref(), Some("+639171234567"));
        assert_eq!(
            string_field(&json!({ "data": { "phone": "+639171234567" } }), "phone").as_deref(),
            Some("+639171234567"),
        );
    }

    #[test]
    fn a_blank_number_is_no_number() {
        assert_eq!(string_field(&json!({ "phone": "   " }), "phone"), None);
        assert_eq!(string_field(&json!({ "phone": null }), "phone"), None);
        assert_eq!(string_field(&json!({}), "phone"), None);
    }

    #[test]
    fn a_finished_job_takes_no_more_messages() {
        for closed in ["delivered", "cancelled", "CANCELED", "returned", "failed"] {
            assert!(!job_is_open(closed), "{closed} should be closed");
        }
        for open in ["pending", "confirmed", "picked_up", "in_transit", "out_for_delivery", "delivery_attempted"] {
            assert!(job_is_open(open), "{open} should be open");
        }
    }
}
