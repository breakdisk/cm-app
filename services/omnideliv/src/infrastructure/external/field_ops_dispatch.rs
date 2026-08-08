//! Offers a placed order to field-ops couriers.
//!
//! OmniDeliv is a product tier and field-ops is the platform tier that owns
//! couriers (ADR-0015), so this is a product calling a platform service —
//! the allowed direction. The `CourierDispatch` trait it implements belongs to
//! OmniDeliv, so the dependency points inward and field-ops knows nothing about
//! this caller.
//!
//! ## Why the token is minted per call rather than configured
//!
//! field-ops guards its operational routes with `require_auth`, which validates
//! a JWT and enforces `exp` (`libs/auth/src/jwt.rs`). A static `SERVICE_TOKEN`
//! in the environment therefore cannot work, for two independent reasons:
//!
//! 1. **It expires.** Any token that validates today stops validating when it
//!    ages out, and a deployment whose checkout silently starts 500ing an hour
//!    after a restart is the worst possible failure shape.
//! 2. **It carries one tenant.** field-ops reads `tenant_id` from the token and
//!    deliberately refuses to take it from the request body, because a
//!    caller-supplied tenant would be no isolation at all. A single fixed token
//!    would therefore offer *every* tenant's orders to *one* tenant's couriers.
//!
//! Signing HS256 costs microseconds, so minting one per request is cheaper than
//! any cache that would have to reason about expiry. The TTL is deliberately
//! tiny: the token exists for the duration of one internal HTTP call.

use std::sync::Arc;

use async_trait::async_trait;
use logisticos_auth::{claims::Claims, jwt::JwtService};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::services::CourierDispatch;

/// Stable identity for the omnideliv service in field-ops' audit trail. Fixed
/// rather than random per call so the actor is greppable, and obviously
/// synthetic so it can never collide with a real `identity.users` row.
const OMNIDELIV_SERVICE_USER: Uuid = Uuid::from_u128(0xf1e1_d0f5_0000_4000_8000_0000_0000_0001);

/// Long enough to cover one internal call and any sane clock skew, short enough
/// that a leaked token is worthless before it can be replayed.
const SERVICE_TOKEN_TTL_SECS: i64 = 60;

pub struct FieldOpsDispatch {
    http:     reqwest::Client,
    base_url: String,
    /// Shared with field-ops via `AUTH__JWT_SECRET`. Holding the signer rather
    /// than a signed string is what lets the tenant vary per call.
    jwt: Arc<JwtService>,
}

impl FieldOpsDispatch {
    pub fn new(base_url: String, jwt: Arc<JwtService>) -> Self {
        Self { http: reqwest::Client::new(), base_url, jwt }
    }

    /// A single-purpose token scoped to one tenant.
    ///
    /// No roles and no permissions: field-ops' offer route reads `tenant_id`
    /// and nothing else, and granting more than the callee checks would be a
    /// standing invitation to widen the blast radius later.
    fn mint(&self, tenant_id: Uuid) -> anyhow::Result<String> {
        let claims = Claims::new(
            OMNIDELIV_SERVICE_USER,
            tenant_id,
            "service".to_string(),
            "internal".to_string(),
            "omnideliv@service.internal".to_string(),
            Vec::new(),
            Vec::new(),
            SERVICE_TOKEN_TTL_SECS,
        );
        self.jwt
            .issue_access_token(claims)
            .map_err(|e| anyhow::anyhow!("could not mint a field-ops service token: {e}"))
    }
}

#[derive(Debug, Deserialize)]
struct SupplyResponse {
    available: usize,
}

#[async_trait]
impl crate::application::services::CourierSupply for FieldOpsDispatch {
    async fn available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<usize> {
        let token = self.mint(tenant_id)?;

        let res = self
            .http
            .get(format!("{}/v1/field-ops/couriers/supply", self.base_url))
            .bearer_auth(token)
            .query(&[("lat", lat), ("lng", lng), ("radius_km", radius_km)])
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("field-ops supply lookup failed: {status} {body}");
        }

        Ok(res.json::<SupplyResponse>().await?.available)
    }
}

#[derive(Debug, Deserialize)]
struct OfferResponse {
    assignment_ids: Vec<Uuid>,
}

#[async_trait]
impl CourierDispatch for FieldOpsDispatch {
    async fn offer(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
        trip_cents: i64,
        tip_cents: i64,
    ) -> anyhow::Result<Vec<Uuid>> {
        // Tenant is not sent in the body: field-ops reads it from the token, and
        // a caller-supplied tenant would let one tenant offer work to another's
        // couriers. The order id travels as `external_ref` — field-ops stores it
        // opaquely and never interprets it, which is what keeps the platform
        // tier product-agnostic.
        let token = self.mint(tenant_id)?;

        let res = self
            .http
            .post(format!("{}/v1/field-ops/assignments/offer", self.base_url))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "product":      "omnideliv",
                "external_ref": order_id,
                "lat":          lat,
                "lng":          lng,
                "trip_cents":   trip_cents,
                "tip_cents":    tip_cents,
            }))
            .send()
            .await?;

        // An auth or routing failure must not read as "no couriers available".
        // Both end in an empty list at the call site, but only one of them is a
        // reason to tell the customer to try again later — the other is an
        // outage that needs to page someone.
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("field-ops offer failed: {status} {body}");
        }

        Ok(res.json::<OfferResponse>().await?.assignment_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch() -> FieldOpsDispatch {
        FieldOpsDispatch::new(
            "http://field-ops:8090".into(),
            Arc::new(JwtService::new("test-secret-at-least-32-chars-long!!", 3600, 86400)),
        )
    }

    /// The property that a static token cannot have: the tenant in the token
    /// follows the caller. Without this, every tenant's orders would be offered
    /// to whichever tenant the configured token happened to name.
    #[test]
    fn the_minted_token_carries_the_callers_tenant() {
        let d = dispatch();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let ca = d.jwt.validate_access_token(&d.mint(a).unwrap()).unwrap().claims;
        let cb = d.jwt.validate_access_token(&d.mint(b).unwrap()).unwrap().claims;

        assert_eq!(ca.tenant_id, a);
        assert_eq!(cb.tenant_id, b);
        assert_ne!(ca.tenant_id, cb.tenant_id);
    }

    /// field-ops validates `exp`, so the token must be short-lived by
    /// construction rather than by whoever pasted it into an env file.
    #[test]
    fn the_minted_token_expires_within_the_ttl() {
        let d = dispatch();
        let c = d.jwt.validate_access_token(&d.mint(Uuid::new_v4()).unwrap()).unwrap().claims;

        let life = c.exp - c.iat;
        assert_eq!(life, SERVICE_TOKEN_TTL_SECS);
        assert!(life <= 300, "a service token good for {life}s is a replay window");
    }

    /// It must validate against the same secret field-ops holds — that shared
    /// secret is the whole trust relationship.
    #[test]
    fn a_token_signed_with_a_different_secret_is_rejected() {
        let d = dispatch();
        let token = d.mint(Uuid::new_v4()).unwrap();

        let other = JwtService::new("a-completely-different-secret-value!!", 3600, 86400);
        assert!(other.validate_access_token(&token).is_err());
    }

    /// Granting no roles keeps the token's authority to exactly what the callee
    /// reads. A future reviewer widening this should have to change a test.
    #[test]
    fn the_service_token_grants_no_roles_or_permissions() {
        let d = dispatch();
        let c = d.jwt.validate_access_token(&d.mint(Uuid::new_v4()).unwrap()).unwrap().claims;

        assert!(c.roles.is_empty());
        assert!(c.permissions.is_empty());
        assert_eq!(c.user_id, OMNIDELIV_SERVICE_USER);
    }
}
