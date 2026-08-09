//! Pushes a mapped catalog into OmniDeliv's ingest port.
//!
//! ## Why the token is minted per call
//!
//! Identical reasoning to `omnideliv`'s own `FieldOpsDispatch`, and worth
//! restating because getting it wrong is silent:
//!
//! 1. **A static token expires.** Any token in the environment that validates
//!    today stops validating later, and a connector that starts 401ing an hour
//!    after a restart is the worst failure shape available.
//! 2. **A static token carries one tenant.** omnideliv reads `tenant_id` from
//!    the token and refuses to take it from the body — a caller-supplied tenant
//!    is no isolation at all — so one fixed token would sync every tenant's
//!    catalog into one tenant's stores.
//!
//! Signing HS256 costs microseconds. A cache would have to reason about expiry,
//! which is more code and more ways to be wrong.
//!
//! ## The permission
//!
//! `catalog:ingest` is what `/v1/omnideliv/internal/catalog/ingest` checks
//! before it will accept a caller-named `vendor_id`. It is granted here and
//! nowhere else — no roles, no other permissions — because the callee checks
//! exactly this and granting more would widen the blast radius of a leak for no
//! benefit.

use std::sync::Arc;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::adapters::shopify_catalog::IngestItem;

/// Stable, obviously synthetic identity for the connectors service in
/// omnideliv's audit trail. Fixed so the actor is greppable; outside any range
/// `identity.users` allocates so it can never collide with a real person.
const CONNECTORS_SERVICE_USER: Uuid = Uuid::from_u128(0xc0_11ec_705_0000_4000_8000_0000_0001);

/// Long enough for one internal call and any sane clock skew, short enough that
/// a leaked token is worthless before it can be replayed.
const SERVICE_TOKEN_TTL_SECS: i64 = 60;

/// The one permission the ingest route checks.
const INGEST_PERMISSION: &str = "catalog:ingest";

#[derive(Debug, Serialize)]
struct IngestRequest<'a> {
    vendor_id: Uuid,
    source:    &'a str,
    items:     &'a [IngestItem],
}

/// What one sync did, straight from omnideliv.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct IngestReport {
    pub created:  usize,
    pub updated:  usize,
    pub rejected: usize,
}

pub struct OmniDelivClient {
    http:     reqwest::Client,
    base_url: String,
    /// Shared with omnideliv via `AUTH__JWT_SECRET`. Holding the signer rather
    /// than a signed string is what lets the tenant vary per call.
    jwt: Arc<JwtService>,
}

impl OmniDelivClient {
    pub fn new(base_url: String, jwt: Arc<JwtService>) -> Self {
        Self { http: reqwest::Client::new(), base_url, jwt }
    }

    fn mint(&self, tenant_id: Uuid, tenant_slug: &str) -> AppResult<String> {
        let claims = Claims::new(
            CONNECTORS_SERVICE_USER,
            tenant_id,
            tenant_slug.to_string(),
            "internal".to_string(),
            "connectors@service.internal".to_string(),
            Vec::new(),                                  // no roles
            vec![INGEST_PERMISSION.to_string()],         // exactly one permission
            SERVICE_TOKEN_TTL_SECS,
        );
        self.jwt.issue_access_token(claims).map_err(|e| AppError::ExternalService {
            service: "omnideliv".into(),
            message: format!("could not mint an ingest token: {e}"),
        })
    }

    /// Apply a mapped catalog to one store.
    ///
    /// Hits the `/internal/` route, which the API gateway refuses to proxy from
    /// outside the cluster — this is a service-to-service call over the mesh and
    /// is not reachable by a merchant's browser.
    ///
    /// `vendor_id` is named by this caller; omnideliv proves it belongs to the
    /// token's tenant before writing anything. Neither side trusts the other's
    /// word about who owns what.
    pub async fn ingest_catalog(
        &self,
        tenant_id: Uuid,
        tenant_slug: &str,
        vendor_id: Uuid,
        source: &str,
        items: &[IngestItem],
    ) -> AppResult<IngestReport> {
        // An empty batch is not a no-op worth a round trip, and — more to the
        // point — a fetch that silently returned nothing must not be reported
        // as a successful sync of zero items.
        if items.is_empty() {
            return Err(AppError::Validation(
                "refusing to sync an empty catalog — the fetch returned no sellable items".into(),
            ));
        }

        let token = self.mint(tenant_id, tenant_slug)?;
        let url = format!("{}/v1/omnideliv/internal/catalog/ingest", self.base_url);

        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&IngestRequest { vendor_id, source, items })
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "omnideliv".into(),
                message: e.to_string(),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body   = resp.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "omnideliv".into(),
                message: format!("catalog ingest failed: HTTP {status} — {body}"),
            });
        }

        resp.json::<IngestReport>().await.map_err(|e| AppError::ExternalService {
            service: "omnideliv".into(),
            message: format!("ingest response was not the expected shape: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> OmniDelivClient {
        OmniDelivClient::new(
            "http://omnideliv:8091".into(),
            Arc::new(JwtService::new("test-secret-that-is-long-enough", 3600, 86400)),
        )
    }

    /// The token carries the permission the callee checks — and nothing else.
    /// A service token that quietly accumulated roles would turn a leaked
    /// 60-second credential into a general-purpose one.
    #[test]
    fn the_minted_token_grants_exactly_the_ingest_permission() {
        let c = client();
        let tenant = Uuid::new_v4();
        let token = c.mint(tenant, "demo").expect("mint");

        let claims = c.jwt.validate_access_token(&token).expect("our own token must validate").claims;
        assert_eq!(claims.permissions, vec!["catalog:ingest".to_string()]);
        assert!(claims.roles.is_empty(), "a sync worker needs no roles");
        assert!(claims.has_permission(INGEST_PERMISSION));
    }

    /// The tenant travels in the token, not the body. This is what stops one
    /// merchant's sync from writing into another tenant's stores.
    #[test]
    fn the_minted_token_carries_the_callers_tenant() {
        let c = client();
        let tenant = Uuid::new_v4();
        let claims = c
            .jwt
            .validate_access_token(&c.mint(tenant, "demo").unwrap())
            .unwrap()
            .claims;

        assert_eq!(claims.tenant_id, tenant);
        assert_eq!(claims.user_id, CONNECTORS_SERVICE_USER);
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let token = client().mint(Uuid::new_v4(), "demo").unwrap();
        let stranger = JwtService::new("a-completely-different-secret-value", 3600, 86400);
        assert!(stranger.validate_access_token(&token).is_err());
    }

    /// A fetch that returned nothing is a failure to report, not a sync of zero
    /// items. Sending it would ask omnideliv to accept an empty batch and would
    /// be logged as a successful sync — the shape in which a broken credential
    /// looks like an empty store.
    #[tokio::test]
    async fn an_empty_catalog_is_refused_rather_than_reported_as_success() {
        let err = client()
            .ingest_catalog(Uuid::new_v4(), "demo", Uuid::new_v4(), "shopify", &[])
            .await;
        assert!(err.is_err(), "an empty batch must not be sent");
    }
}
