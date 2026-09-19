//! A home move's survey photos. The lead (or staff) records them; the
//! customer, the lead and staff see them. The typed inventory prices the move
//! — photos are its visual context: condition evidence against a false damage
//! claim, and access constraints so no crew is blindsided on the day.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use serde::Deserialize;
use uuid::Uuid;

use super::home_move::{record_for, Party};
use super::AppState;
use crate::infrastructure::db::home_photos::{self, NewPhoto};

/// Photos a move may carry.
pub const MAX_PHOTOS: i64 = 60;
const MAX_BYTES: i64 = 5 * 1_048_576;

#[derive(Debug, Deserialize)]
pub struct AddPhoto {
    /// condition | access
    pub kind: String,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub caption: String,
    /// The key pod issued the upload for.
    pub object_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    /// The shutter, on the lead's phone.
    #[serde(default)]
    pub device_timestamp: Option<DateTime<Utc>>,
}

/// Why a photo would be refused, or None. The key must be the one pod issued
/// for this tenant's move — no recording another move's image, or any other
/// object in the store.
pub fn photo_problem(p: &AddPhoto, tenant_id: Uuid, shipment_id: Uuid) -> Option<String> {
    if !matches!(p.kind.as_str(), "condition" | "access") {
        return Some("A survey photo is condition evidence or an access constraint".into());
    }
    let prefix = format!("survey/{tenant_id}/{shipment_id}/");
    if !p.object_key.starts_with(&prefix) || p.object_key.contains("..") || p.object_key.len() > 200 {
        return Some("That upload is not this move's".into());
    }
    if !matches!(p.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Some("A survey photo is a JPEG, PNG or WebP".into());
    }
    if !(1..=MAX_BYTES).contains(&p.size_bytes) {
        return Some("A photo is at most 5 MB".into());
    }
    if p.caption.chars().count() > 200 || p.room.as_deref().is_some_and(|r| r.len() > 40) {
        return Some("Keep the caption under 200 characters".into());
    }
    None
}

/// `POST /v1/shipments/:id/home/photos`
pub async fn add(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<AddPhoto>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let (_, party) = record_for(&s, &claims, id).await?;
    if matches!(party, Party::Customer) {
        return Err(AppError::Forbidden { resource: "survey photos".into() });
    }
    if let Some(p) = photo_problem(&req, claims.tenant_id, id) {
        return Err(AppError::Validation(p));
    }
    if home_photos::count(&s.pool, id).await.map_err(AppError::Internal)? >= MAX_PHOTOS {
        return Err(AppError::BusinessRule(format!("A move holds at most {MAX_PHOTOS} survey photos")));
    }
    let photo = home_photos::insert(&s.pool, &NewPhoto {
        shipment_id: id,
        tenant_id: claims.tenant_id,
        uploaded_by: claims.user_id,
        kind: &req.kind,
        room: req.room.as_deref().filter(|r| !r.trim().is_empty()),
        caption: req.caption.trim(),
        object_key: &req.object_key,
        content_type: &req.content_type,
        size_bytes: req.size_bytes,
        device_timestamp: req.device_timestamp,
    })
    .await
    .map_err(AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": photo }))))
}

/// `GET /v1/shipments/:id/home/photos` — each with a URL good for an hour.
pub async fn list(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    record_for(&s, &claims, id).await?;
    let mut photos = home_photos::list(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?;
    if !photos.is_empty() {
        let keys: Vec<String> = photos.iter().map(|p| p.object_key.clone()).collect();
        match s.svc.home_teams.media_urls(&keys).await {
            Ok(urls) => {
                for p in &mut photos {
                    p.url = urls.get(&p.object_key).cloned();
                }
            }
            // Listed without pictures rather than not at all.
            Err(e) => tracing::warn!(shipment_id = %id, err = %e, "survey photo URLs unavailable"),
        }
    }
    Ok(Json(serde_json::json!({ "data": photos })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(key: &str) -> AddPhoto {
        AddPhoto {
            kind: "condition".into(),
            room: Some("living".into()),
            caption: "Scratch on the left arm".into(),
            object_key: key.into(),
            content_type: "image/jpeg".into(),
            size_bytes: 400_000,
            device_timestamp: None,
        }
    }

    #[test]
    fn only_this_moves_uploads_are_recorded() {
        let (t, sh) = (Uuid::new_v4(), Uuid::new_v4());
        let mine = format!("survey/{t}/{sh}/{}.jpg", Uuid::new_v4());
        assert_eq!(photo_problem(&photo(&mine), t, sh), None);
        // Another move's image, the POD store, a path escape: refused.
        assert!(photo_problem(&photo(&format!("survey/{t}/{}/x.jpg", Uuid::new_v4())), t, sh).is_some());
        assert!(photo_problem(&photo(&format!("pod/{t}/{sh}/x.jpg")), t, sh).is_some());
        assert!(photo_problem(&photo(&format!("survey/{t}/{sh}/../../pod/x.jpg")), t, sh).is_some());
    }

    #[test]
    fn a_photo_is_condition_or_access_and_an_image() {
        let (t, sh) = (Uuid::new_v4(), Uuid::new_v4());
        let key = format!("survey/{t}/{sh}/a.jpg");
        assert!(photo_problem(&AddPhoto { kind: "access".into(), ..photo(&key) }, t, sh).is_none());
        assert!(photo_problem(&AddPhoto { kind: "selfie".into(), ..photo(&key) }, t, sh).is_some());
        assert!(photo_problem(&AddPhoto { content_type: "application/pdf".into(), ..photo(&key) }, t, sh).is_some());
        assert!(photo_problem(&AddPhoto { size_bytes: 0, ..photo(&key) }, t, sh).is_some());
        assert!(photo_problem(&AddPhoto { size_bytes: 6 * 1_048_576, ..photo(&key) }, t, sh).is_some());
    }
}
