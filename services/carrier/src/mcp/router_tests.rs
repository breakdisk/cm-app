//! End-to-end tests of the carrier MCP surface: a real router, a real JWT, and
//! a 3PL double that records every call that reaches it. An empty record means
//! nothing was executed.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use logisticos_auth::{claims::Claims, rbac::permissions};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use super::router::mcp_router;
use crate::infrastructure::adapters::{
    AdapterError, AdapterRegistry, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingInfo,
};
use crate::test_support::{app_state, Bookings, Carriers, Market};

const SECRET: &str = "mcp-test-secret";

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

#[async_trait]
impl CarrierAdapter for Recorder {
    fn carrier_code(&self) -> &'static str { "DHL" }
    fn carrier_name(&self) -> &'static str { "Recorder" }
    async fn get_rates(&self, _r: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        self.0.lock().unwrap().push("rates".into());
        Ok(vec![])
    }
    async fn book_shipment(&self, _r: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        self.0.lock().unwrap().push("book".into());
        Ok(BookingConfirmation {
            booking_ref: "BK1".into(), tracking_number: "TN1".into(),
            label_url: None, label_bytes: None, estimated_delivery: None,
        })
    }
    async fn track_shipment(&self, t: &str) -> Result<TrackingInfo, AdapterError> {
        self.0.lock().unwrap().push(format!("track:{t}"));
        Ok(TrackingInfo {
            tracking_number: t.into(), current_status: "ok".into(),
            current_location: None, estimated_delivery: None, events: vec![],
        })
    }
    async fn cancel_shipment(&self, r: &str) -> Result<(), AdapterError> {
        self.0.lock().unwrap().push(format!("cancel:{r}"));
        Ok(())
    }
    async fn get_label(&self, r: &str, _f: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        self.0.lock().unwrap().push(format!("label:{r}"));
        Ok(vec![1, 2, 3])
    }
}

struct Harness {
    app:      Router,
    recorder: Arc<Recorder>,
    jwt:      Arc<logisticos_auth::jwt::JwtService>,
    /// The tenant `token()` signs for, and the owner of booking BK1 / TN1.
    tenant:   Uuid,
    bookings: Arc<Bookings>,
}

fn harness() -> Harness {
    let recorder = Arc::new(Recorder::default());
    let mut adapters = AdapterRegistry::new();
    adapters.register(recorder.clone());
    let tenant = Uuid::new_v4();
    let bookings = Arc::new(Bookings::default());
    bookings.seed(tenant, "DHL", "BK1", "TN1");
    let state = app_state(
        Arc::new(Carriers::default()), Arc::new(Market::default()), adapters, SECRET,
        Some(bookings.clone()),
    );
    let jwt = state.jwt.clone();
    Harness { app: mcp_router().with_state(state), recorder, jwt, tenant, bookings }
}

impl Harness {
    fn token(&self, perms: &[&str], partner: Option<Uuid>) -> String {
        self.token_for(self.tenant, perms, partner)
    }

    fn token_for(&self, tenant: Uuid, perms: &[&str], partner: Option<Uuid>) -> String {
        let claims = Claims::new(
            Uuid::new_v4(), tenant, "t".into(), "enterprise".into(), "u@t.test".into(),
            vec![], perms.iter().map(|p| (*p).to_string()).collect(), 300,
        ).with_partner(partner);
        self.jwt.issue_access_token(claims).expect("token")
    }

    async fn call(&self, token: &str, tool: &str, args: Value) -> (StatusCode, Value) {
        let body = json!({ "params": { "name": tool, "arguments": args } });
        let req = Request::builder().method(Method::POST).uri("/mcp/tools/call")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(body.to_string())).expect("request");
        let r = self.app.clone().oneshot(req).await.expect("response");
        let s = r.status();
        let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("body");
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    async fn list(&self, token: &str) -> Vec<String> {
        let req = Request::builder().uri("/mcp/tools")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty()).expect("request");
        let r = self.app.clone().oneshot(req).await.expect("response");
        let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("body");
        let v: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
        v["tools"].as_array().map(|a| {
            a.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect()
        }).unwrap_or_default()
    }

    fn calls(&self) -> Vec<String> { self.recorder.0.lock().unwrap().clone() }
}

fn address() -> Value {
    json!({"name":"A","street1":"1 St","city":"Manila","postal_code":"1000","country":"PH"})
}

/// Valid arguments for every tool, so a refusal is about permission, not shape.
fn valid_args(tool: &str) -> Value {
    match tool {
        "get_carrier_rates" => json!({"origin": address(), "destination": address(), "weight_kg": 1.0, "carrier_code": "DHL"}),
        "book_carrier_shipment" => json!({"carrier_code":"DHL","service_code":"S","shipper":address(),"consignee":address(),"weight_kg":1.0}),
        "track_carrier_shipment" => json!({"tracking_number":"TN1","carrier_code":"DHL"}),
        "cancel_carrier_shipment" => json!({"carrier_code":"DHL","booking_ref":"BK1"}),
        "get_carrier_label" => json!({"carrier_code":"DHL","booking_ref":"BK1"}),
        _ => json!({}),
    }
}

const ALL_TOOLS: [&str; 5] = [
    "get_carrier_rates", "book_carrier_shipment", "track_carrier_shipment",
    "cancel_carrier_shipment", "get_carrier_label",
];

#[tokio::test]
async fn a_caller_with_no_permissions_is_refused_everywhere_and_nothing_runs() {
    let h = harness();
    let t = h.token(&[], None);
    for tool in ALL_TOOLS {
        let (s, _) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{tool}");
    }
    assert!(h.calls().is_empty(), "a refused call reached the carrier: {:?}", h.calls());
}

#[tokio::test]
async fn read_permission_runs_the_read_tools_and_is_refused_the_write_tools() {
    let h = harness();
    let t = h.token(&[permissions::CARRIERS_READ], None);
    for tool in ["get_carrier_rates", "track_carrier_shipment", "get_carrier_label"] {
        let (s, b) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::OK, "{tool}: {b}");
    }
    assert_eq!(h.calls().len(), 3);
    for tool in ["book_carrier_shipment", "cancel_carrier_shipment"] {
        let (s, _) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{tool}");
    }
    assert_eq!(h.calls().len(), 3, "a refused write reached the carrier");
}

#[tokio::test]
async fn manage_permission_runs_book_and_cancel() {
    let h = harness();
    let t = h.token(&[permissions::CARRIERS_MANAGE], None);
    let (s, b) = h.call(&t, "book_carrier_shipment", valid_args("book_carrier_shipment")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = h.call(&t, "cancel_carrier_shipment", valid_args("cancel_carrier_shipment")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(h.calls(), vec!["book".to_string(), "cancel:BK1".to_string()]);
}

#[tokio::test]
async fn manage_own_alone_cannot_book_or_cancel() {
    let h = harness();
    let t = h.token(&[permissions::CARRIERS_READ, permissions::CARRIERS_MANAGE_OWN], None);
    for tool in ["book_carrier_shipment", "cancel_carrier_shipment"] {
        let (s, _) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{tool}");
    }
    assert!(h.calls().is_empty());
}

#[tokio::test]
async fn a_partner_bound_token_cannot_reach_bookings_even_with_the_permission() {
    let h = harness();
    let t = h.token(&[permissions::CARRIERS_READ, permissions::CARRIERS_MANAGE], Some(Uuid::new_v4()));
    for tool in ["track_carrier_shipment", "get_carrier_label", "book_carrier_shipment", "cancel_carrier_shipment"] {
        let (s, _) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{tool}");
    }
    assert!(h.calls().is_empty());
}

#[tokio::test]
async fn an_unknown_tool_is_refused_not_executed() {
    let h = harness();
    let t = h.token(&["*"], None);
    let (s, _) = h.call(&t, "drop_all_tables", json!({})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tool_listing_shows_only_what_the_caller_may_run() {
    let h = harness();
    assert!(h.list(&h.token(&[], None)).await.is_empty());
    let mut read = h.list(&h.token(&[permissions::CARRIERS_READ], None)).await;
    read.sort();
    assert_eq!(read, ["get_carrier_label", "get_carrier_rates", "track_carrier_shipment"]);
    assert_eq!(h.list(&h.token(&[permissions::CARRIERS_MANAGE], None)).await.len(), 2);
    assert_eq!(h.list(&h.token(&["*"], None)).await.len(), 5);
}

#[tokio::test]
async fn a_bad_token_is_401() {
    let h = harness();
    let (s, _) = h.call("not-a-jwt", "get_carrier_rates", valid_args("get_carrier_rates")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// ── Identifier validation ─────────────────────────────────────────────────

#[tokio::test]
async fn path_and_query_smuggling_in_references_never_reaches_the_carrier() {
    let h = harness();
    let t = h.token(&["*"], None);
    for bad in ["../internal/x", "BK1?tenant_id=other", "a/b", "BK1#x", ".."] {
        for (tool, field) in [
            ("cancel_carrier_shipment", "booking_ref"),
            ("get_carrier_label", "booking_ref"),
            ("track_carrier_shipment", "tracking_number"),
        ] {
            let mut args = valid_args(tool);
            args[field] = json!(bad);
            let (s, _) = h.call(&t, tool, args).await;
            assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{tool} {bad}");
        }
    }
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[tokio::test]
async fn track_without_a_carrier_code_validates_before_fanning_out() {
    let h = harness();
    let t = h.token(&["*"], None);
    let (s, _) = h.call(&t, "track_carrier_shipment", json!({"tracking_number": "../x"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(h.calls().is_empty());
}

#[tokio::test]
async fn a_malformed_shipment_id_rejects_the_booking_instead_of_dropping_it() {
    let h = harness();
    let t = h.token(&["*"], None);
    let mut args = valid_args("book_carrier_shipment");
    args["shipment_id"] = json!("../internal/x");
    let (s, _) = h.call(&t, "book_carrier_shipment", args).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(h.calls().is_empty(), "booked with a shipment_id that was then discarded");
}

// ── Tenant ownership of references ────────────────────────────────────────

#[tokio::test]
async fn another_tenants_reference_is_not_found_and_never_reaches_the_carrier() {
    let h = harness();
    let stranger = h.token_for(Uuid::new_v4(), &["*"], None);
    for tool in ["cancel_carrier_shipment", "get_carrier_label", "track_carrier_shipment"] {
        // BK1 / TN1 belong to the harness tenant, not to this caller.
        let (s, b) = h.call(&stranger, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{tool}");
        assert!(b["error"].as_str().is_some_and(|e| e.contains("No booking")), "{tool}: {b}");
        // Without a carrier_code the answer must be the same (no fan-out probe).
        let mut args = valid_args(tool);
        if let Some(o) = args.as_object_mut() { o.remove("carrier_code"); }
        let (s, _) = h.call(&stranger, tool, args).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{tool} without carrier_code");
    }
    assert!(h.calls().is_empty(), "a foreign reference reached the carrier: {:?}", h.calls());
}

#[tokio::test]
async fn an_unknown_reference_reads_exactly_like_a_foreign_one() {
    let h = harness();
    let t = h.token(&["*"], None);
    let (s1, b1) = h.call(&t, "cancel_carrier_shipment", json!({"carrier_code":"DHL","booking_ref":"NOPE"})).await;
    let stranger = h.token_for(Uuid::new_v4(), &["*"], None);
    let (s2, b2) = h.call(&stranger, "cancel_carrier_shipment", valid_args("cancel_carrier_shipment")).await;
    assert_eq!((s1, &b1), (s2, &b2));
    assert!(h.calls().is_empty());
}

#[tokio::test]
async fn the_wrong_carrier_for_an_own_reference_is_not_found() {
    let h = harness();
    let t = h.token(&["*"], None);
    let (s, _) = h.call(&t, "cancel_carrier_shipment", json!({"carrier_code":"UPS","booking_ref":"BK1"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(h.calls().is_empty());
}

#[tokio::test]
async fn own_references_are_allowed() {
    let h = harness();
    let t = h.token(&["*"], None);
    for tool in ["cancel_carrier_shipment", "get_carrier_label", "track_carrier_shipment"] {
        let (s, b) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::OK, "{tool}: {b}");
    }
    // Carrier code omitted: taken from the booking record.
    let (s, b) = h.call(&t, "track_carrier_shipment", json!({"tracking_number":"TN1"})).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["result"]["carrier_code"], "DHL");
    assert_eq!(h.calls().len(), 4);
}

#[tokio::test]
async fn a_booking_made_over_mcp_is_recorded_for_the_booker() {
    let h = harness();
    h.bookings.0.lock().unwrap().clear();
    let t = h.token(&["*"], None);
    let (s, b) = h.call(&t, "book_carrier_shipment", valid_args("book_carrier_shipment")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let stranger = h.token_for(Uuid::new_v4(), &["*"], None);
    let (s, _) = h.call(&stranger, "cancel_carrier_shipment", valid_args("cancel_carrier_shipment")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = h.call(&t, "cancel_carrier_shipment", valid_args("cancel_carrier_shipment")).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn with_no_booking_store_every_reference_is_refused() {
    let recorder = Arc::new(Recorder::default());
    let mut adapters = AdapterRegistry::new();
    adapters.register(recorder.clone());
    let state = app_state(Arc::new(Carriers::default()), Arc::new(Market::default()), adapters, SECRET, None);
    let jwt = state.jwt.clone();
    let h = Harness { app: mcp_router().with_state(state), recorder, jwt, tenant: Uuid::new_v4(), bookings: Arc::new(Bookings::default()) };
    let t = h.token(&["*"], None);
    for tool in ["cancel_carrier_shipment", "get_carrier_label", "track_carrier_shipment"] {
        let (s, _) = h.call(&t, tool, valid_args(tool)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{tool}");
    }
    assert!(h.calls().is_empty());
}
