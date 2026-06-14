use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;
use logisticos_events::{envelope::Event, topics};

use crate::application::services::{ProfileService, UpsertProfileCommand};
use crate::domain::entities::ProfileType;
use crate::domain::repositories::ProfileFilter;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/customers",                                                  get(list_profiles))
        .route("/v1/customers/top-clv",                                          get(top_by_clv))
        .route("/v1/customers/:external_id",                                     get(get_profile).put(upsert_profile))
        .route("/v1/customers/:external_id/events",                              get(get_events))
        .route("/v1/customers/:external_id/churn-score",                         get(get_churn_score))
        .route("/v1/customers/:external_id/preferences",                         get(get_preferences))
        .route("/v1/customers/:external_id/support-tickets",                     post(open_support_ticket))
        .route("/v1/customers/:external_id/support-tickets/:ticket_id/close",    post(close_support_ticket))
}

// ---------------------------------------------------------------------------
// GET /v1/customers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    name:         Option<String>,
    email:        Option<String>,
    phone:        Option<String>,
    min_clv:      Option<f32>,
    profile_type: Option<String>,
    limit:        Option<i64>,
    offset:       Option<i64>,
}

async fn list_profiles(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let filter = ProfileFilter {
        name_contains: q.name,
        email:         q.email,
        phone:         q.phone,
        min_clv:       q.min_clv,
        profile_type:  q.profile_type,
        limit:         q.limit.unwrap_or(50).clamp(1, 200),
        offset:        q.offset.unwrap_or(0).max(0),
    };

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profiles = state
        .profile_svc
        .list(&tenant_id, filter)
        .await?;
    let count = profiles.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"profiles": profiles, "count": count}))))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/top-clv
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TopQuery {
    limit: Option<i64>,
}

async fn top_by_clv(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<TopQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profiles = state
        .profile_svc
        .top_by_clv(&tenant_id, q.limit.unwrap_or(20))
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"profiles": profiles}))))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id
// ---------------------------------------------------------------------------

async fn get_profile(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(profile)))
}

// ---------------------------------------------------------------------------
// PUT /v1/customers/:external_id — upsert identity fields
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpsertBody {
    pub name:         Option<String>,
    pub email:        Option<String>,
    pub phone:        Option<String>,
    pub profile_type: Option<String>,
}

async fn upsert_profile(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
    Json(body): Json<UpsertBody>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_MANAGE)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile_type = body.profile_type.as_deref().and_then(|s| match s {
        "sender"   => Some(ProfileType::Sender),
        "receiver" => Some(ProfileType::Receiver),
        _          => None,
    });

    let summary = state
        .profile_svc
        .upsert(
            &tenant_id,
            UpsertProfileCommand {
                external_customer_id: external_id,
                name:                 body.name,
                email:                body.email,
                phone:                body.phone,
                profile_type,
            },
        )
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(summary)))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id/events — recent behavioral events
// ---------------------------------------------------------------------------

async fn get_events(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "external_customer_id": external_id,
            "events": profile.recent_events,
            "count": profile.recent_events.len(),
        })),
    ))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id/churn-score
// Derives a churn probability (0.0–1.0) from CLV + engagement + recency signals.
// ---------------------------------------------------------------------------

async fn get_churn_score(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    // Heuristic churn score:
    //   Low CLV + low engagement + no recent shipment → high churn risk.
    //   Score is inverted from the engagement/CLV normalised values.
    let clv_factor = 1.0 - (profile.clv_score / 100.0).clamp(0.0, 1.0);
    let eng_factor = 1.0 - (profile.engagement_score / 100.0).clamp(0.0, 1.0);
    let recency_factor = profile.last_shipment_at.map(|t| {
        let days = (chrono::Utc::now() - t).num_days() as f32;
        (days / 180.0).clamp(0.0, 1.0)
    }).unwrap_or(1.0);

    let churn_score = ((clv_factor * 0.4) + (eng_factor * 0.35) + (recency_factor * 0.25))
        .clamp(0.0, 1.0);

    let risk_tier = match churn_score {
        s if s >= 0.7 => "high",
        s if s >= 0.4 => "medium",
        _             => "low",
    };

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "external_customer_id": external_id,
            "churn_score": churn_score,
            "risk_tier": risk_tier,
            "signals": {
                "clv_score": profile.clv_score,
                "engagement_score": profile.engagement_score,
                "last_shipment_at": profile.last_shipment_at,
                "total_shipments": profile.total_shipments,
            }
        })),
    ))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id/preferences
// Returns communication channel preferences inferred from profile data.
// ---------------------------------------------------------------------------

async fn get_preferences(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    // Infer preferred channel from available contact data.
    let preferred_channel = if profile.phone.is_some() {
        "whatsapp"
    } else if profile.email.is_some() {
        "email"
    } else {
        "push"
    };

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "external_customer_id": external_id,
            "preferred_channel": preferred_channel,
            "opt_in": {
                "whatsapp": profile.phone.is_some(),
                "sms":      profile.phone.is_some(),
                "email":    profile.email.is_some(),
                "push":     true,
            },
            "language": "en",
            "delivery_time_window": {
                "preferred_from_hour": 9,
                "preferred_to_hour":   18,
            }
        })),
    ))
}

// ---------------------------------------------------------------------------
// POST /v1/customers/:external_id/support-tickets
// Opens a support ticket and emits SUPPORT_TICKET_OPENED so the engagement
// engine suppresses campaign messages until the ticket is closed.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenTicketBody {
    subject:  Option<String>,
    message:  Option<String>,
}

async fn open_support_ticket(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
    Json(body): Json<OpenTicketBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CUSTOMERS_MANAGE)?;

    let ticket_id = Uuid::new_v4();

    let event = Event::new(
        "logisticos/cdp",
        "support.ticket.opened",
        claims.tenant_id,
        serde_json::json!({
            "ticket_id":   ticket_id,
            "customer_id": external_id,
            "tenant_id":   claims.tenant_id,
            "subject":     body.subject.unwrap_or_default(),
            "message":     body.message.unwrap_or_default(),
            "opened_at":   Utc::now().to_rfc3339(),
        }),
    );

    if let Err(e) = state.kafka.publish_event(topics::SUPPORT_TICKET_OPENED, &event).await {
        tracing::warn!(err = %e, ticket_id = %ticket_id, "Failed to publish SUPPORT_TICKET_OPENED");
    }

    Ok::<_, AppError>((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "ticket_id":   ticket_id,
            "customer_id": external_id,
            "status":      "open",
        })),
    ))
}

// ---------------------------------------------------------------------------
// POST /v1/customers/:external_id/support-tickets/:ticket_id/close
// Closes a support ticket and emits SUPPORT_TICKET_CLOSED so the engagement
// engine lifts the campaign suppression flag for this customer.
// ---------------------------------------------------------------------------

async fn close_support_ticket(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path((external_id, ticket_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CUSTOMERS_MANAGE)?;

    let event = Event::new(
        "logisticos/cdp",
        "support.ticket.closed",
        claims.tenant_id,
        serde_json::json!({
            "ticket_id":   ticket_id,
            "customer_id": external_id,
            "tenant_id":   claims.tenant_id,
            "closed_at":   Utc::now().to_rfc3339(),
        }),
    );

    if let Err(e) = state.kafka.publish_event(topics::SUPPORT_TICKET_CLOSED, &event).await {
        tracing::warn!(err = %e, ticket_id = %ticket_id, "Failed to publish SUPPORT_TICKET_CLOSED");
    }

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "ticket_id":   ticket_id,
            "customer_id": external_id,
            "status":      "closed",
        })),
    ))
}
