//! Mesh-internal reads for whole-home moves: the teams each date has
//! (driver-ops) and the lead who reserved a move (dispatch). No JWT: these
//! routes are internal, refused by the gateway, mTLS in the mesh.

use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::value_objects::home_move::DayCapacity;

pub struct HomeTeamsClient {
    driver_ops_url: Option<String>,
    dispatch_url: Option<String>,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

/// Pay credited to a driver outside a delivered task — driver-ops'
/// `EarningCredit`, net of commission.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LeadCredit {
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub kind: &'static str,
    pub reference_id: Uuid,
    pub tracking_number: Option<String>,
    pub gross_cents: i64,
    pub commission_cents: i64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reservation {
    pub driver_id: Uuid,
    pub lead_name: String,
    pub move_date: NaiveDate,
}

impl HomeTeamsClient {
    pub fn new(driver_ops_url: Option<String>, dispatch_url: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(4))
            .build()
            .unwrap_or_default();
        Self { driver_ops_url, dispatch_url, http }
    }

    /// None when driver-ops is not configured; an error when it fails.
    pub async fn capacity(&self, tenant_id: Uuid, from: NaiveDate, to: NaiveDate) -> anyhow::Result<Option<Vec<DayCapacity>>> {
        let Some(base) = self.driver_ops_url.as_deref() else { return Ok(None) };
        let url = format!("{}/v1/internal/home-leads/capacity", base.trim_end_matches('/'));
        let resp = self
            .http
            .get(url)
            .query(&[("tenant_id", tenant_id.to_string()), ("from", from.to_string()), ("to", to.to_string())])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("driver-ops capacity returned {}", resp.status());
        }
        Ok(Some(resp.json::<Envelope<Vec<DayCapacity>>>().await?.data))
    }

    /// Credit a lead; safe to repeat (driver-ops credits each kind of work
    /// on a shipment once). Ok(false) when driver-ops is not configured.
    pub async fn credit(&self, credit: &LeadCredit) -> anyhow::Result<bool> {
        let Some(base) = self.driver_ops_url.as_deref() else { return Ok(false) };
        let url = format!("{}/v1/internal/earnings/credits", base.trim_end_matches('/'));
        let resp = self.http.post(url).json(credit).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("driver-ops credit returned {}", resp.status());
        }
        Ok(true)
    }

    /// The lead who reserved this move; None before one has.
    pub async fn reservation(&self, shipment_id: Uuid) -> anyhow::Result<Option<Reservation>> {
        let Some(base) = self.dispatch_url.as_deref() else { return Ok(None) };
        let url = format!("{}/v1/internal/home-reservations/{shipment_id}", base.trim_end_matches('/'));
        let resp = self.http.get(url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            anyhow::bail!("dispatch reservation returned {}", resp.status());
        }
        Ok(Some(resp.json::<Envelope<Reservation>>().await?.data))
    }
}
