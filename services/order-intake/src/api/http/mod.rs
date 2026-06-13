use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;
use sqlx::{PgPool, Row};

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::{
    commands::{BulkCreateShipmentCommand, CancelShipmentCommand, CreateShipmentCommand, InternalCreateShipmentCommand, RescheduleShipmentCommand},
    queries::ShipmentQueryService,
    services::shipment_service::ShipmentService,
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub svc:    Arc<ShipmentService>,
    pub query:  Arc<ShipmentQueryService>,
    pub jwt:    Arc<logisticos_auth::jwt::JwtService>,
    pub pool:   PgPool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn create_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(mut cmd): Json<CreateShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_CREATE)?;
    cmd.tenant_id   = claims.tenant_id;
    cmd.merchant_id = claims.user_id; // merchant uses their user UUID as merchant_id
    // Derive 3-char AWB tenant code from the tenant slug (first 3 alphanumeric chars, uppercased)
    if cmd.tenant_code.is_empty() {
        cmd.tenant_code = claims.tenant_slug
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I')
            .take(3)
            .collect::<String>()
            .to_uppercase();
    }
    // Mark as customer-booked when the JWT role is "customer" (customer app self-booking).
    let is_customer = claims.roles.contains(&"customer".to_string());
    let is_merchant = claims.roles.contains(&"merchant".to_string())
        || claims.roles.contains(&"merchant_admin".to_string())
        || claims.roles.contains(&"merchant_user".to_string());
    if is_customer {
        cmd.booked_by_customer = true;
    }
    // Default auto_dispatch by role unless the caller set it explicitly.
    // All roles default to true — dispatch fails gracefully if no driver is
    // available and the shipment remains in the queue for manual assignment.
    let is_admin = claims.roles.iter().any(|r| r == "admin" || r == "ops");
    let auto_dispatch_effective = cmd.auto_dispatch.unwrap_or(is_customer || is_merchant || is_admin);
    cmd.auto_dispatch = Some(auto_dispatch_effective);
    // Customer-booked shipments need a deliverable email for the POD payment
    // receipt. If the booking didn't include one, fall back to the JWT email
    // (authenticated customer profile). Merchant-booked flows keep email
    // optional — merchants get invoiced, not receipted.
    if cmd.booked_by_customer {
        let has_valid = cmd.customer_email.as_deref()
            .map(str::trim)
            .is_some_and(|e| !e.is_empty() && e.contains('@'));
        if !has_valid {
            let jwt_email = claims.email.trim();
            if !jwt_email.is_empty() && jwt_email.contains('@') {
                cmd.customer_email = Some(jwt_email.to_owned());
            } else {
                return Err(AppError::Validation(
                    "A customer email is required for self-booked shipments (needed for payment receipt delivery)".into(),
                ));
            }
        }
    }
    tracing::info!(
        service_type   = %cmd.service_type,
        origin_country = %cmd.origin.country_code,
        dest_country   = %cmd.destination.country_code,
        tenant_slug    = %claims.tenant_slug,
        tenant_code    = %cmd.tenant_code,
        weight_grams   = cmd.weight_grams,
        booked_by_customer = cmd.booked_by_customer,
        auto_dispatch      = auto_dispatch_effective,
        "create_shipment handler entered",
    );
    match s.svc.create(cmd).await {
        Ok(shipment) => Ok::<_, AppError>((StatusCode::CREATED, Json(shipment))),
        Err(e) => {
            tracing::error!(error = ?e, "create_shipment handler: service returned error");
            Err(e)
        }
    }
}

async fn bulk_create_shipments(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(mut cmd): Json<BulkCreateShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_CREATE)?;
    cmd.tenant_id   = claims.tenant_id;
    cmd.merchant_id = claims.user_id;
    // Stamp tenant context onto each row. Bulk rows arrive carrying only shipment
    // details — tenant_id, merchant_id, and the AWB tenant_code come from the JWT,
    // mirroring the per-field injection the single-create handler performs.
    let derived_tenant_code = claims.tenant_slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I')
        .take(3)
        .collect::<String>()
        .to_uppercase();
    for row in cmd.rows.iter_mut() {
        row.tenant_id   = claims.tenant_id;
        row.merchant_id = claims.user_id;
        if row.tenant_code.is_empty() {
            row.tenant_code = derived_tenant_code.clone();
        }
    }
    let result = s.svc.bulk_create(cmd).await?;
    Ok::<_, AppError>((StatusCode::MULTI_STATUS, Json(result)))
}

async fn get_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let shipment = s.query.get_by_id(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(shipment)))
}

async fn cancel_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(mut cmd): Json<CancelShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    cmd.shipment_id = id;
    s.svc.cancel(cmd).await?;
    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}

async fn reschedule_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(mut cmd): Json<RescheduleShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    cmd.shipment_id = id;
    s.svc.reschedule(cmd).await?;
    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}

#[derive(serde::Deserialize)]
struct OverrideStatusBody {
    status: String,
    #[allow(dead_code)]
    reason: Option<String>,
}

async fn admin_override_status(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<OverrideStatusBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    // Map string → ShipmentStatus
    use logisticos_types::ShipmentStatus;
    let new_status = match body.status.as_str() {
        "pending"            => ShipmentStatus::Pending,
        "confirmed"          => ShipmentStatus::Confirmed,
        "pickup_assigned"    => ShipmentStatus::PickupAssigned,
        "picked_up"          => ShipmentStatus::PickedUp,
        "in_transit"         => ShipmentStatus::InTransit,
        "at_hub"             => ShipmentStatus::AtHub,
        "out_for_delivery"   => ShipmentStatus::OutForDelivery,
        "delivery_attempted" => ShipmentStatus::DeliveryAttempted,
        "delivered"          => ShipmentStatus::Delivered,
        "partial_delivery"   => ShipmentStatus::PartialDelivery,
        "piece_exception"    => ShipmentStatus::PieceException,
        "customs_hold"       => ShipmentStatus::CustomsHold,
        "failed"             => ShipmentStatus::Failed,
        "cancelled"          => ShipmentStatus::Cancelled,
        "returned"           => ShipmentStatus::Returned,
        other => return Err(AppError::Validation(format!("Unknown status: {other}"))),
    };
    let actor = claims.user_id.to_string();

    // Capture the prior status before the override so the timeline event records
    // an accurate from → to transition. Best-effort: a lookup miss just yields a
    // NULL from_status (manual override is authoritative regardless).
    let from_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM order_intake.shipments WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(claims.tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    s.svc.override_status(id, new_status, &actor).await?;

    // Stamp the manual transition into the immutable timeline (date/time/location).
    let _ = sqlx::query(
        r#"INSERT INTO order_intake.shipment_events
               (tenant_id, shipment_id, event_type, from_status, to_status, actor_id, actor_type, metadata)
           SELECT $1, $2, 'status_changed', $3, $4, $5, 'user',
                  jsonb_build_object(
                      'location',
                      CASE WHEN $4 IN ('pending','confirmed','pickup_assigned','picked_up')
                           THEN s.origin_city ELSE s.dest_city END,
                      'reason', $6::text,
                      'override', true
                  )
           FROM order_intake.shipments s
           WHERE s.id = $2"#,
    )
    .bind(claims.tenant_id)
    .bind(id)
    .bind(from_status)
    .bind(body.status)
    .bind(claims.user_id)
    .bind(body.reason)
    .execute(&s.pool)
    .await
    .map_err(|e| tracing::warn!(error = %e, shipment_id = %id, "override status event insert failed (status updated)"));

    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}

// ---------------------------------------------------------------------------
// Timeline (shipment_events) — read side
// ---------------------------------------------------------------------------

/// Human-readable label for a destination status, derived at read time so it
/// stays i18n-able and decoupled from the stored event rows.
fn status_description(to_status: &str) -> &'static str {
    match to_status {
        "pending"            => "Shipment created",
        "confirmed"          => "Shipment confirmed",
        "pickup_assigned"    => "Pickup assigned to courier",
        "picked_up"          => "Parcel picked up by courier",
        "in_transit"         => "In transit",
        "at_hub"             => "Arrived at sorting hub",
        "out_for_delivery"   => "Out for delivery",
        "delivery_attempted" => "Delivery attempted",
        "delivered"          => "Delivered",
        "partial_delivery"   => "Partially delivered",
        "piece_exception"    => "Piece exception reported",
        "customs_hold"       => "Held at customs",
        "failed"             => "Delivery failed",
        "cancelled"          => "Shipment cancelled",
        "returned"           => "Returned to sender",
        _                    => "Status updated",
    }
}

/// Shape one `shipment_events` row into the timeline JSON consumed by both the
/// admin detail panel and the public tracking page. `metadata.location` is the
/// city-level label stamped at write time.
fn event_row_to_json(r: &sqlx::postgres::PgRow) -> serde_json::Value {
    let to_status: String = r.get("to_status");
    let from_status: Option<String> = r.get("from_status");
    let event_type: String = r.get("event_type");
    let actor_type: Option<String> = r.get("actor_type");
    let metadata: serde_json::Value = r.get("metadata");
    let occurred_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
    let location = metadata.get("location").and_then(|v| v.as_str()).map(str::to_owned);

    serde_json::json!({
        "status":      to_status,
        "from_status": from_status,
        "to_status":   to_status,
        "event_type":  event_type,
        "actor_type":  actor_type,
        "description": status_description(&to_status),
        "location":    location,
        "occurred_at": occurred_at,
        "metadata":    metadata,
    })
}

/// `GET /v1/shipments/:id/events` — authenticated timeline for the admin/ops
/// shipment detail panel. Tenant-scoped.
async fn list_shipment_events(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let rows = sqlx::query(
        r#"SELECT event_type, from_status, to_status, actor_type, metadata, created_at
             FROM order_intake.shipment_events
            WHERE shipment_id = $1 AND tenant_id = $2
            ORDER BY created_at ASC"#,
    )
    .bind(id)
    .bind(claims.tenant_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let events: Vec<serde_json::Value> = rows.iter().map(event_row_to_json).collect();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "events": events }))))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(state: AppState) -> Router {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    // Authenticated /v1 routes (JWT required via auth_layer).
    let authed = Router::new()
        .route("/shipments",        post(create_shipment).get(list_shipments))
        .route("/shipments/bulk",   post(bulk_create_shipments))
        .route("/shipments/:id",    get(get_shipment))
        .route("/shipments/:id/events",     get(list_shipment_events))
        .route("/shipments/:id/cancel",     post(cancel_shipment))
        .route("/shipments/:id/reschedule", post(reschedule_shipment))
        .route("/shipments/:id/status",     put(admin_override_status))
        .layer(auth_layer);

    Router::new()
        .route("/health", get(|| async { axum::Json(serde_json::json!({"status":"ok","service":"order-intake"})) }))
        .nest("/v1", authed)
        // Internal service-to-service endpoints — no JWT auth required (Istio mTLS enforces caller identity).
        .nest("/v1/internal", Router::new()
            .route("/shipments",             post(internal_create_shipment))
            .route("/shipments/dims",        get(batch_shipment_dims))
            .route("/shipments/:id/billing", get(get_shipment_billing))
            .route("/billing/shipments",     get(list_billing_shipments))
        )
        .with_state(state)
}

/// Internal endpoint called by the connectors service after verifying platform HMAC.
/// No JWT required — Istio mTLS enforces that only internal services can reach this.
async fn internal_create_shipment(
    State(s): State<AppState>,
    Json(cmd): Json<InternalCreateShipmentCommand>,
) -> impl IntoResponse {
    // Derive AWB tenant code from slug (same algorithm as the authenticated handler).
    let tenant_code: String = cmd.tenant_slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I')
        .take(3)
        .collect::<String>()
        .to_uppercase();

    let create_cmd = CreateShipmentCommand {
        tenant_id:         cmd.tenant_id,
        merchant_id:       cmd.merchant_id,
        tenant_code,
        customer_name:     cmd.customer_name,
        customer_phone:    cmd.customer_phone,
        customer_email:    cmd.customer_email,
        origin:            cmd.origin,
        destination:       cmd.destination,
        service_type:      cmd.service_type,
        weight_grams:      cmd.weight_grams,
        length_cm:         cmd.length_cm,
        width_cm:          cmd.width_cm,
        height_cm:         cmd.height_cm,
        declared_value_cents: cmd.declared_value_cents,
        cod_amount_cents:  cmd.cod_amount_cents,
        special_instructions: cmd.special_instructions,
        merchant_reference: cmd.merchant_reference,
        description:       cmd.description,
        source_platform:   Some(cmd.source_platform),
        external_order_id: cmd.external_order_id,
        piece_count:       None,
        booked_by_customer: false,
        auto_dispatch:     Some(true),
        merchant_name:     None,
        delivery_category: None,
    };

    match s.svc.create(create_cmd).await {
        Ok(shipment) => Ok::<_, AppError>((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id":  shipment.id.inner(),
                "awb": shipment.awb.as_str(),
                "status": "pending",
            })),
        )),
        Err(e) => {
            tracing::error!(error = ?e, "internal_create_shipment: service error");
            Err(e)
        }
    }
}

async fn get_shipment_billing(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let shipment = s.query.get_by_id(id).await?;

    let base_freight   = shipment.compute_base_fee();
    let fuel_surcharge = (base_freight.amount as f64 * 0.05).round() as i64; // 5% fuel levy
    let insurance      = shipment.declared_value
        .map(|v| (v.amount as f64 * 0.005).round() as i64) // 0.5% of declared value
        .unwrap_or(0);
    let total = base_freight.amount + fuel_surcharge + insurance;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "shipment_id":          id,
        "awb":                  shipment.awb.as_str(),
        "merchant_id":          shipment.merchant_id.inner(),
        "currency":             format!("{:?}", base_freight.currency),
        "base_freight":         base_freight.amount,
        "fuel_surcharge":       fuel_surcharge,
        "insurance":            insurance,
        "total":                total,
    }))))
}

async fn list_shipments(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<crate::application::queries::ListShipmentsQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let (shipments, total) = s.query.list(claims.tenant_id, q).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "shipments": shipments,
        "total": total,
    }))))
}

async fn list_billing_shipments(
    State(s): State<AppState>,
    Query(q): Query<crate::application::queries::BillingShipmentsQuery>,
) -> impl IntoResponse {
    let shipments = s.query.list_for_billing(q).await?;
    let count = shipments.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "shipments": shipments,
        "count":     count,
    }))))
}

/// `GET /v1/internal/shipments/dims`
///
/// Internal endpoint — no JWT required (Istio mTLS enforces caller identity).
/// Returns declared weight and dimensions for a batch of shipment IDs so that
/// hub-ops can enrich the consolidation manifest with real box sizes.
///
/// Query params: `?shipment_ids=<uuid>&shipment_ids=<uuid>...` (up to 500)
#[derive(Deserialize)]
struct DimsQuery {
    #[serde(default)]
    shipment_ids: Vec<Uuid>,
}

#[derive(sqlx::FromRow)]
struct DimsRow {
    shipment_id: Uuid,
    awb:         String,
    weight_g:    i32,
    length_cm:   Option<i32>,
    width_cm:    Option<i32>,
    height_cm:   Option<i32>,
}

async fn batch_shipment_dims(
    State(s): State<AppState>,
    Query(q): Query<DimsQuery>,
) -> impl IntoResponse {
    if q.shipment_ids.is_empty() {
        return Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "dims": [] }))));
    }

    // Cap to 500 IDs per call — prevents accidental large queries.
    let ids: Vec<Uuid> = q.shipment_ids.into_iter().take(500).collect();

    let rows = sqlx::query_as::<_, DimsRow>(
        r#"SELECT id          AS shipment_id,
                  awb,
                  weight_grams AS weight_g,
                  length_cm,
                  width_cm,
                  height_cm
             FROM order_intake.shipments
            WHERE id = ANY($1)"#,
    )
    .bind(&ids)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let dims: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "shipment_id": r.shipment_id,
        "awb":         r.awb,
        "weight_g":    r.weight_g,
        "length_cm":   r.length_cm,
        "width_cm":    r.width_cm,
        "height_cm":   r.height_cm,
    })).collect();

    Ok((StatusCode::OK, Json(serde_json::json!({ "dims": dims }))))
}
