//! Screen B's transport.
//!
//! Server-Sent Events, not WebSockets: the orchestration window is seconds long
//! and strictly server→client, so a persistent bidirectional socket would add
//! sticky sessions, reconnect handling and mobile background-socket behaviour
//! for no benefit. The stream closes when the run ends.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::post,
    Json, Router,
};
use futures_util::stream::Stream;
use logisticos_auth::middleware::AuthClaims;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub utterance: String,
    /// Where the customer is. Required: every vendor search in the run is
    /// centred here, so a missing point is not a detail to default away — it
    /// would return plausible shops in the wrong place, which reads as the
    /// agent working badly rather than as a missing field.
    pub delivery_lat: f64,
    pub delivery_lng: f64,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/mesh/run", post(run))
}

async fn run(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<RunRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Buffered so a slow client cannot block a specialist. If the client
    // disconnects the run still completes and the basket is persisted — the
    // customer can reopen it rather than losing the work.
    let (tx, rx) = mpsc::channel(64);

    let mesh = st.mesh.clone();
    let utterance = req.utterance;
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let customer_id = claims.user_id;

    let (lat, lng) = (req.delivery_lat, req.delivery_lng);

    tokio::spawn(async move {
        mesh.run(tenant_id, customer_id, utterance, lat, lng, tx).await;
    });

    let stream = ReceiverStream::new(rx).map(|ev| {
        Ok(Event::default()
            .json_data(&ev)
            .unwrap_or_else(|_| Event::default().data("{\"event\":\"failed\",\"reason\":\"serialisation\"}")))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
