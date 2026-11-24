use axum::{extract::State, Json};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    api::http::AppState,
    domain::{
        entities::{NewPartnerMembership, PartnerMembership, MEMBERSHIP_ROLES},
        repositories::UpsertOutcome,
    },
};

#[derive(Debug, Deserialize)]
pub struct UpsertPartnerMembershipRequest {
    pub tenant_id:  Uuid,
    pub user_id:    Uuid,
    pub carrier_id: Uuid,
    pub role:       String,
}

fn validate_role(role: &str) -> Result<(), AppError> {
    if MEMBERSHIP_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "role must be one of {}", MEMBERSHIP_ROLES.join(", ")
        )))
    }
}

/// A suspended row is a conflict, not a success: the caller asked for an active
/// membership and must not believe it got one.
fn outcome_to_result(outcome: UpsertOutcome) -> Result<PartnerMembership, AppError> {
    match outcome {
        UpsertOutcome::Active(m) => Ok(m),
        UpsertOutcome::Suspended(_) => Err(AppError::Conflict(
            "this user's membership for that partner is suspended; reactivate it deliberately".into(),
        )),
        UpsertOutcome::ConflictsWith(_) => Err(AppError::Conflict(
            "this user already acts for a different partner".into(),
        )),
    }
}

/// `GET /v1/partner-memberships/me`: the partner the caller acts for.
pub async fn get_my(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let membership = state
        .partner_membership_repo
        .find_active(claims.tenant_id, claims.user_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound { resource: "PartnerMembership", id: claims.user_id.to_string() })?;
    Ok(Json(serde_json::json!({ "data": membership })))
}

/// `POST /v1/internal/partner-memberships`: create a membership. Mesh-internal,
/// guarded by the internal secret and blocked at the gateway. Idempotent on
/// `(tenant_id, user_id, carrier_id)`.
pub async fn upsert_internal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpsertPartnerMembershipRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_role(&req.role)?;
    let outcome = state
        .partner_membership_repo
        .upsert_active(&NewPartnerMembership {
            tenant_id: req.tenant_id,
            user_id: req.user_id,
            carrier_id: req.carrier_id,
            role: req.role,
        })
        .await
        .map_err(AppError::Internal)?;
    let membership = outcome_to_result(outcome)?;
    tracing::info!(
        tenant_id = %membership.tenant_id, user_id = %membership.user_id,
        carrier_id = %membership.carrier_id, "partner membership active"
    );
    Ok(Json(serde_json::json!({ "data": membership })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{entities::PartnerMembership, repositories::UpsertOutcome};
    use chrono::Utc;
    use logisticos_errors::AppError;
    use uuid::Uuid;

    fn membership(status: &str) -> PartnerMembership {
        PartnerMembership {
            id: Uuid::new_v4(), tenant_id: Uuid::new_v4(), user_id: Uuid::new_v4(),
            carrier_id: Uuid::new_v4(), role: "store_manager".into(),
            status: status.into(), created_at: Utc::now(),
        }
    }

    #[test]
    fn known_membership_roles_are_accepted() {
        for role in ["partner", "store_manager", "store_clerk", "store_rider"] {
            assert!(validate_role(role).is_ok(), "{role}");
        }
    }

    #[test]
    fn a_platform_role_is_not_a_membership_role() {
        for role in ["admin", "tenant_admin", "dispatcher", "driver", "", "STORE_MANAGER"] {
            assert!(matches!(validate_role(role), Err(AppError::Validation(_))), "{role:?}");
        }
    }

    #[test]
    fn an_active_membership_is_returned() {
        let m = membership("active");
        assert_eq!(outcome_to_result(UpsertOutcome::Active(m.clone())).unwrap(), m);
    }

    #[test]
    fn a_suspended_membership_is_a_conflict_not_a_success() {
        let err = outcome_to_result(UpsertOutcome::Suspended(membership("suspended"))).unwrap_err();
        assert!(matches!(err, AppError::Conflict(ref m) if m.contains("suspended")), "{err:?}");
    }

    #[test]
    fn another_partner_is_a_conflict() {
        let err = outcome_to_result(UpsertOutcome::ConflictsWith(Uuid::new_v4())).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)), "{err:?}");
    }
}
