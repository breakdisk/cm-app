//! Network International (NI) hosted-checkout adapter.
//!
//! LogisticOS never collects card data directly — the customer is redirected
//! to NI's own hosted payment page. This keeps `services/payments` at PCI
//! SAQ-A instead of pulling it into full PCI scope.
//!
//! NOTE: the exact request/response JSON shapes and webhook signature header
//! name below follow NI's (N-Genius) publicly documented hosted-order pattern
//! as of this writing. Confirm field names and the signature scheme against
//! NI's live API/sandbox docs during integration testing before going live —
//! this is a deliberate, stated boundary: it fixes the contract our own
//! services expose, not NI's wire format, which needs verification against a
//! real sandbox this plan cannot obtain.

use async_trait::async_trait;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::config::NetworkInternationalConfig;
use crate::domain::repositories::payment_gateway::{
    CreateSessionRequest, GatewaySession, PaymentGateway, WebhookEvent,
};

type HmacSha256 = Hmac<Sha256>;

pub struct NetworkInternationalGateway {
    cfg: NetworkInternationalConfig,
    http: reqwest::Client,
}

impl NetworkInternationalGateway {
    pub fn new(cfg: NetworkInternationalConfig) -> Self {
        Self { cfg, http: reqwest::Client::new() }
    }
}

#[derive(Serialize)]
struct CreateOrderRequest<'a> {
    action: &'a str,
    amount: OrderAmount<'a>,
    merchant_order_reference: String,
    merchant_attributes: MerchantAttributes<'a>,
}

#[derive(Serialize)]
struct OrderAmount<'a> {
    #[serde(rename = "currencyCode")]
    currency_code: &'a str,
    value: i64,
}

#[derive(Serialize)]
struct MerchantAttributes<'a> {
    #[serde(rename = "redirectUrl")]
    redirect_url: &'a str,
}

#[derive(Deserialize)]
struct CreateOrderResponse {
    reference: String,
    #[serde(rename = "_links")]
    links: OrderLinks,
}

#[derive(Deserialize)]
struct OrderLinks {
    payment: LinkHref,
}

#[derive(Deserialize)]
struct LinkHref {
    href: String,
}

#[async_trait]
impl PaymentGateway for NetworkInternationalGateway {
    async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
        let url = format!(
            "{}/transactions/outlets/{}/orders",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.outlet_ref,
        );
        let body = CreateOrderRequest {
            action: "SALE",
            amount: OrderAmount { currency_code: req.currency, value: req.amount_cents },
            merchant_order_reference: req.intent_id.to_string(),
            merchant_attributes: MerchantAttributes { redirect_url: req.return_url },
        };
        let resp = self.http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<CreateOrderResponse>()
            .await?;

        Ok(GatewaySession {
            checkout_url: resp.links.payment.href,
            gateway_order_ref: resp.reference,
        })
    }

    fn verify_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
        let signature_b64 = headers
            .get("x-ni-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("missing webhook signature header"))?;

        let expected = base64::engine::general_purpose::STANDARD
            .decode(signature_b64)
            .map_err(|_| anyhow::anyhow!("malformed webhook signature"))?;

        let mut mac = HmacSha256::new_from_slice(self.cfg.webhook_secret.as_bytes())
            .map_err(|_| anyhow::anyhow!("invalid webhook secret length"))?;
        mac.update(raw_body);
        let computed = mac.finalize().into_bytes();

        if computed.as_slice().ct_eq(&expected).unwrap_u8() != 1 {
            anyhow::bail!("webhook signature verification failed");
        }

        let payload: WebhookPayload = serde_json::from_slice(raw_body)?;
        Ok(match payload.status.as_str() {
            "CAPTURED" | "AUTHORISED" => WebhookEvent::Captured {
                gateway_order_ref: payload.order_reference,
                gateway_payment_ref: payload.transaction_reference,
            },
            _ => WebhookEvent::Failed { gateway_order_ref: payload.order_reference },
        })
    }

    async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()> {
        let url = format!(
            "{}/transactions/{}/refund",
            self.cfg.base_url.trim_end_matches('/'),
            gateway_payment_ref,
        );
        self.http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&serde_json::json!({ "amount": { "value": amount_cents } }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct WebhookPayload {
    status: String,
    #[serde(rename = "orderReference")]
    order_reference: String,
    #[serde(rename = "transactionReference")]
    transaction_reference: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gateway() -> NetworkInternationalGateway {
        NetworkInternationalGateway::new(NetworkInternationalConfig {
            base_url: "https://example.invalid".into(),
            api_key: "test-key".into(),
            webhook_secret: "test-secret".into(),
            outlet_ref: "outlet-1".into(),
        })
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_webhook_accepts_a_correctly_signed_captured_payload() {
        let gateway = test_gateway();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-1");
                assert_eq!(gateway_payment_ref, "txn-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn verify_webhook_rejects_a_tampered_body() {
        let gateway = test_gateway();
        let signed_body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", signed_body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let tampered = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-EVIL"}"#;
        assert!(gateway.verify_webhook(&headers, tampered).is_err());
    }

    #[test]
    fn verify_webhook_rejects_a_missing_signature_header() {
        let gateway = test_gateway();
        let headers = reqwest::header::HeaderMap::new();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        assert!(gateway.verify_webhook(&headers, body).is_err());
    }

    #[test]
    fn verify_webhook_maps_a_non_captured_status_to_failed() {
        let gateway = test_gateway();
        let body = br#"{"status":"DECLINED","orderReference":"ord-2","transactionReference":"txn-2"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        match gateway.verify_webhook(&headers, body).expect("must verify") {
            WebhookEvent::Failed { gateway_order_ref } => assert_eq!(gateway_order_ref, "ord-2"),
            _ => panic!("expected Failed"),
        }
    }
}
