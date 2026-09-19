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
    /// The primary lead: sole, or a joint move's captain.
    pub driver_id: Uuid,
    pub lead_name: String,
    pub move_date: NaiveDate,
    /// sole | captain. Absent from an older dispatch: sole.
    #[serde(default = "sole")]
    pub role: String,
    /// Every lead on the move, the primary first.
    #[serde(default)]
    pub slots: Vec<SlotLead>,
}

fn sole() -> String {
    "sole".into()
}

/// One lead's part of a move, as dispatch holds it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SlotLead {
    pub driver_id: Uuid,
    #[serde(default)]
    pub lead_name: String,
    /// sole | captain | support | emergency
    pub role: String,
    #[serde(default)]
    pub payout_cents: Option<i64>,
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

    /// Ask dispatch to offer extra trucks on a move (an Emergency Secondary
    /// Dispatch, or a joint move's support slot again). Ok(false) when
    /// dispatch is not configured.
    pub async fn side_slots(&self, tenant_id: Uuid, shipment_id: Uuid, slot: &str, count: i64, payout_cents: Option<i64>) -> anyhow::Result<bool> {
        let Some(base) = self.dispatch_url.as_deref() else { return Ok(false) };
        let url = format!("{}/v1/internal/home-moves/{shipment_id}/slots", base.trim_end_matches('/'));
        let resp = self
            .http
            .post(url)
            .json(&serde_json::json!({ "tenant_id": tenant_id, "slot": slot, "count": count, "payout_cents": payout_cents }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("dispatch side slots returned {}", resp.status());
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
