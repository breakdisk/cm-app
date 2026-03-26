//! WebSocket endpoint for real-time driver location streaming.
//!
//! Connection flow:
//!   1. Client connects: `GET /ws/locations?token=<JWT>`
//!   2. Server validates JWT — closes with 4001 if invalid.
//!   3. Server subscribes client to the broadcast channel.
//!   4. Every location update for the caller's tenant is pushed as a JSON message.
//!   5. Client can send `{"type":"ping"}` — server echoes `{"type":"pong"}`.
//!
//! The merchant portal live dispatch map subscribes here to animate driver beacons.

use axum::{
    extract::{State, WebSocketUpgrade, Query},
    response::IntoResponse,
};
use axum::extract::ws::{WebSocket, Message};
use std::sync::Arc;
use serde::Deserialize;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

pub async fn handle_ws_upgrade(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Validate JWT before upgrading — reject unauthenticated connections at handshake time.
    let claims = match state.jwt.validate_access_token(&params.token) {
        Ok(data) => data.claims,
        Err(_) => {
            // Return HTTP 401 — WebSocket upgrade rejected
            return (axum::http::StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response();
        }
    };

    let tenant_id = claims.tenant_id;
    let location_rx = state.location_tx.subscribe();

    ws.on_upgrade(move |socket| handle_ws(socket, tenant_id, location_rx))
        .into_response()
}

async fn handle_ws(
    mut socket: WebSocket,
    tenant_id: uuid::Uuid,
    mut location_rx: tokio::sync::broadcast::Receiver<crate::api::http::LocationBroadcast>,
) {
    loop {
        tokio::select! {
            // Forward location updates to this WebSocket client (filtered by tenant)
            Ok(broadcast) = location_rx.recv() => {
                if broadcast.tenant_id != tenant_id {
                    continue; // Not for this tenant — skip
                }
                let msg = match serde_json::to_string(&broadcast) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("WS serialize error: {e}");
                        continue;
                    }
                };
                if socket.send(Message::Text(msg)).await.is_err() {
                    break; // Client disconnected
                }
            }

            // Handle messages from client (ping/keepalive)
            Some(Ok(msg)) = socket.recv() => {
                match msg {
                    Message::Text(text) => {
                        // Simple ping/pong keepalive
                        if text.contains("\"ping\"") {
                            let _ = socket.send(Message::Text(r#"{"type":"pong"}"#.to_string())).await;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {} // Ignore binary frames
                }
            }

            else => break, // Channel closed or socket closed
        }
    }

    tracing::debug!(tenant_id = %tenant_id, "WebSocket client disconnected");
}
