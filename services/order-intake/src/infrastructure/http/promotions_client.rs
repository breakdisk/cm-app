//! HTTP client for promotions' mesh-internal pricing and redemption.
//!
//! No JWT: promotions mounts `/v1/internal/promotions/*` outside its auth layer,
//! the gateway refuses every `/internal/` path, and Istio mTLS asserts the
//! caller. The account is sent explicitly — it is the booking user from this
//! service's own validated token, never something the app supplied.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::value_objects::quote_token::TokenDiscount;

pub struct PromotionsClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct PriceRequest<'a> {
    tenant_id: Uuid,
    account_id: Uuid,
    code: Option<&'a str>,
    currency: &'a str,
    carriage_cents: i64,
    accessorial_cents: i64,
    loyalty_enabled: bool,
}

/// One discount on the quote, as promotions priced it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiscountLine {
    /// "code" | "corporate" | "tier" | "credit".
    pub kind: String,
    pub label: String,
    pub amount_cents: i64,
    /// The ceiling cut this line short.
    pub clipped: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Priced {
    pub lines: Vec<DiscountLine>,
    pub total_off_cents: i64,
    pub ceiling_binds: bool,
    /// Set when the code took something off; only then is it redeemed.
    pub code_applied: Option<String>,
    pub refusal: Option<String>,
    pub message: Option<String>,
    /// The code was valid, but the account's corporate rate took more.
    #[serde(default)]
    pub code_lost_to_corporate: bool,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Serialize)]
struct RedeemRequest<'a> {
    tenant_id: Uuid,
    account_id: Uuid,
    shipment_id: Uuid,
    currency: &'a str,
    code: Option<&'a str>,
    lines: &'a [TokenDiscount],
}

#[derive(Serialize)]
struct ReleaseRequest {
    shipment_id: Uuid,
}

/// Why a redemption did not go through.
#[derive(Debug, PartialEq, Eq)]
pub enum RedeemError {
    /// The ledger refused it: this month's code, or a once-per-account code,
    /// was already spent — usually by another booking since the quote.
    Refused(String),
    /// The account's credit no longer covers the credit line, spent by
    /// another booking since the quote.
    CreditChanged,
    /// Promotions could not be reached or failed.
    Unavailable(String),
}

impl PromotionsClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("promotions HTTP client");
        Self { base_url: base_url.into(), http }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url.trim_end_matches('/'))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn price(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        code: Option<&str>,
        currency: &str,
        carriage_cents: i64,
        accessorial_cents: i64,
        loyalty_enabled: bool,
    ) -> Result<Priced, String> {
        let resp = self
            .http
            .post(self.url("/v1/internal/promotions/price"))
            .json(&PriceRequest { tenant_id, account_id, code, currency, carriage_cents, accessorial_cents, loyalty_enabled })
            .send()
            .await
            .map_err(|e| format!("promotions price request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("promotions price returned {}", resp.status()));
        }
        resp.json::<Envelope<Priced>>()
            .await
            .map(|e| e.data)
            .map_err(|e| format!("promotions price response unreadable: {e}"))
    }

    /// Spend every line of a booking together: the code, the credit, and the
    /// tier and corporate lines recorded against their budgets.
    pub async fn redeem(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        shipment_id: Uuid,
        currency: &str,
        code: Option<&str>,
        lines: &[TokenDiscount],
    ) -> Result<(), RedeemError> {
        let resp = self
            .http
            .post(self.url("/v1/internal/promotions/redeem"))
            .json(&RedeemRequest { tenant_id, account_id, shipment_id, currency, code, lines })
            .send()
            .await
            .map_err(|e| RedeemError::Unavailable(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::CONFLICT && body.contains("CREDIT_CHANGED") {
            Err(RedeemError::CreditChanged)
        } else if status == reqwest::StatusCode::CONFLICT {
            Err(RedeemError::Refused(body))
        } else {
            Err(RedeemError::Unavailable(format!("{status}: {body}")))
        }
    }

    /// Best-effort: a booking that failed after spending its code and credit
    /// gives them back. A miss is left to the `shipment.cancelled` path or ops.
    pub async fn release(&self, shipment_id: Uuid) {
        let sent = self
            .http
            .post(self.url("/v1/internal/promotions/release"))
            .json(&ReleaseRequest { shipment_id })
            .send()
            .await;
        match sent {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => tracing::error!(%shipment_id, status = %r.status(), "promotions release refused — the code stays spent"),
            Err(e) => tracing::error!(%shipment_id, err = %e, "promotions release unreachable — the code stays spent"),
        }
    }
}
