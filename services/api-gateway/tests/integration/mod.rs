//! End-to-end tests of the real gateway: `bootstrap::run()` is started exactly
//! as `main` starts it (config from the environment, real router, real auth,
//! real proxy), in front of a stub upstream that echoes what it receives.
//!
//! Using `run()` rather than a hand-assembled router is deliberate: the
//! request handler is private and the router is built inline in `run()`, so
//! this is the only way to mount the middleware exactly as production does.
//!
//! Redis is pointed at a closed port. The gateway fails OPEN on a Redis error
//! (documented in `proxy_handler`), so every request below exercises that
//! path; the 429 branch needs a live Redis and is not covered here.
//!
//! `run()` initialises global tracing and reads process-global env vars, so
//! there is exactly one gateway per test binary, shared by every test.

use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpListener},
    sync::OnceLock,
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::any,
    Json, Router,
};
use logisticos_auth::{claims::Claims, jwt::JwtService};
use serde_json::{json, Value};
use uuid::Uuid;

const SECRET: &str = "integration-test-secret-integration-test-secret";

struct Harness {
    gateway: String,
    jwt: JwtService,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port()
}

/// A stub upstream: echoes method, path, query, headers (every value of each)
/// and body. `/status/<code>` answers with that status and an `x-upstream` header.
async fn echo(req: Request) -> axum::response::Response {
    let method = req.method().to_string();
    let uri = req.uri().clone();
    let headers: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in req.headers() {
            m.entry(k.as_str().to_owned())
                .or_default()
                .push(v.to_str().unwrap_or("<non-utf8>").to_owned());
        }
        m
    };
    let body = axum::body::to_bytes(req.into_body(), 1 << 20).await.unwrap_or_else(|_| Bytes::new());

    if let Some(code) = uri.path().rsplit_once("/status/").and_then(|(_, c)| c.parse::<u16>().ok()) {
        let mut h = HeaderMap::new();
        h.insert("x-upstream", "yes".parse().expect("hv"));
        return (StatusCode::from_u16(code).expect("status"), h, "upstream said so").into_response();
    }

    Json(json!({
        "method": method,
        "path": uri.path(),
        "query": uri.query(),
        "headers": headers,
        "body": String::from_utf8_lossy(&body),
    }))
    .into_response()
}

fn start() -> Harness {
    let upstream_port = free_port();
    let gateway_port = free_port();
    let closed_port = free_port(); // bound then dropped: nothing listens

    // Every service gets a distinct path prefix on the one stub, so the echoed
    // path tells us which upstream the gateway chose.
    let up = format!("http://127.0.0.1:{upstream_port}");
    let services = [
        "identity", "cdp", "engagement", "order_intake", "dispatch", "driver_ops",
        "delivery_experience", "fleet", "hub_ops", "carrier", "pod", "payments",
        "analytics", "marketing", "business_logic", "ai_layer",
    ];
    for s in services {
        std::env::set_var(format!("SERVICES__{}_URL", s.to_uppercase()), format!("{up}/{s}"));
    }
    std::env::set_var("SERVICES__COMPLIANCE_URL", format!("{up}/compliance"));
    // field-ops is configured but dead, to exercise the 502 path.
    std::env::set_var("SERVICES__FIELD_OPS_URL", format!("http://127.0.0.1:{closed_port}"));
    std::env::set_var("SERVICES__OMNIDELIV_URL", format!("{up}/omnideliv"));
    // promotions / webhooks / connectors deliberately unset.
    for k in ["SERVICES__PROMOTIONS_URL", "SERVICES__WEBHOOKS_URL", "SERVICES__CONNECTORS_URL"] {
        std::env::remove_var(k);
    }

    std::env::set_var("APP__HOST", "127.0.0.1");
    std::env::set_var("APP__PORT", gateway_port.to_string());
    std::env::set_var("APP__ENV", "test");
    std::env::set_var("APP__CORS_ALLOWED_ORIGINS", "*");
    std::env::set_var("AUTH__JWT_SECRET", SECRET);
    std::env::set_var("AUTH__JWT_EXPIRY_SECONDS", "3600");
    std::env::set_var("AUTH__REFRESH_TOKEN_EXPIRY_SECONDS", "86400");
    std::env::set_var("REDIS__URL", format!("redis://127.0.0.1:{closed_port}/"));

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", upstream_port))
                .await
                .expect("bind upstream");
            tokio::spawn(async move {
                axum::serve(listener, Router::new().fallback(any(echo))).await.ok();
            });
            if let Err(e) = logisticos_api_gateway::bootstrap::run().await {
                panic!("gateway exited: {e:#}");
            }
        });
    });

    let addr: SocketAddr = format!("127.0.0.1:{gateway_port}").parse().expect("addr");
    for _ in 0..200 {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return Harness {
                gateway: format!("http://{addr}"),
                jwt: JwtService::new(SECRET, 3600, 86_400),
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("gateway never started listening on {addr}");
}

fn h() -> &'static Harness {
    HARNESS.get_or_init(start)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("client")
}

fn claims(tier: &str, tenant: Uuid) -> Claims {
    Claims::new(
        Uuid::new_v4(),
        tenant,
        "demo".into(),
        tier.into(),
        "u@example.com".into(),
        vec!["admin".into()],
        vec![],
        3600,
    )
}

fn token(tier: &str, tenant: Uuid) -> String {
    h().jwt.issue_access_token(claims(tier, tenant)).expect("token")
}

fn diner_token(tenant: Uuid) -> String {
    h().jwt
        .issue_access_token(Claims::for_table_session(Uuid::new_v4(), tenant, "demo".into(), "starter".into(), 600))
        .expect("diner token")
}

async fn get(path: &str, bearer: Option<&str>) -> reqwest::Response {
    let mut r = client().get(format!("{}{path}", h().gateway));
    if let Some(t) = bearer {
        r = r.bearer_auth(t);
    }
    r.send().await.expect("gateway reachable")
}

async fn echoed(resp: reqwest::Response) -> Value {
    assert_eq!(resp.status(), 200, "expected the stub upstream to answer");
    resp.json().await.expect("echo json")
}

// ── authentication ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_protected_path_without_a_token_is_401_and_never_reaches_upstream() {
    let r = get("/v1/shipments", None).await;
    assert_eq!(r.status(), 401);
    let body: Value = r.json().await.expect("json");
    assert_eq!(body["error"], "Missing Authorization header");
}

#[tokio::test]
async fn a_malformed_or_foreign_token_is_401() {
    // Not a JWT at all, wrong scheme, and a well-formed JWT signed with a
    // different secret.
    let foreign = JwtService::new("some-other-secret-some-other-secret-xx", 3600, 3600)
        .issue_access_token(claims("starter", Uuid::new_v4()))
        .expect("token");
    for bearer in ["garbage", "a.b.c", foreign.as_str()] {
        let r = get("/v1/shipments", Some(bearer)).await;
        assert_eq!(r.status(), 401, "{bearer}");
        let body: Value = r.json().await.expect("json");
        assert_eq!(body["error"], "Invalid or expired token");
    }
    let r = client()
        .get(format!("{}/v1/shipments", h().gateway))
        .header("Authorization", format!("Basic {}", token("starter", Uuid::new_v4())))
        .send()
        .await
        .expect("send");
    assert_eq!(r.status(), 401, "only the Bearer scheme is accepted");
}

#[tokio::test]
async fn an_expired_token_is_401() {
    let mut c = claims("starter", Uuid::new_v4());
    c.iat -= 7200;
    c.exp -= 7200;
    let t = h().jwt.issue_access_token(c).expect("token");
    assert_eq!(get("/v1/shipments", Some(&t)).await.status(), 401);
}

#[tokio::test]
async fn a_refresh_token_is_not_an_access_token() {
    use logisticos_auth::claims::RefreshClaims;
    let rc = RefreshClaims::new(Uuid::new_v4(), Uuid::new_v4(), 3600);
    let t = h().jwt.issue_refresh_token(rc).expect("refresh token");
    assert_eq!(get("/v1/shipments", Some(&t)).await.status(), 401);
}

// ── public paths ────────────────────────────────────────────────────────

#[tokio::test]
async fn public_paths_skip_auth_and_are_forwarded_as_tenant_public() {
    for (path, upstream) in [
        ("/v1/auth/login", "identity"),
        ("/v1/auth/otp/verify", "identity"),
        ("/v1/public/branding", "identity"),
        ("/v1/tracking/public/CM-1", "delivery_experience"),
        ("/v1/omnideliv/public/catalog/a/items/b/photo", "omnideliv"),
        ("/v1/omnideliv/tables/tok123/session", "omnideliv"),
    ] {
        let v = echoed(get(path, None).await).await;
        assert_eq!(v["path"], format!("/{upstream}{path}"), "{path}");
        assert_eq!(v["headers"]["x-tenant-id"], json!(["public"]), "{path}");
    }
}

#[tokio::test]
async fn the_public_allowlist_is_exact_not_a_prefix() {
    for path in [
        "/v1/auth/login/extra",
        "/v1/auth/me",
        "/v1/auth/loginx",
        "/v1/tracking/publicx/CM-1",
        "/v1/tracking/CM-1",
        "/v1/omnideliv/tables/tok123/orders",
        "/v1/payments/intents",
    ] {
        assert_eq!(get(path, None).await.status(), 401, "{path} must require a token");
    }
}

// ── authenticated forwarding ────────────────────────────────────────────

#[tokio::test]
async fn a_valid_token_is_forwarded_with_the_tenant_from_the_claims() {
    let tenant = Uuid::new_v4();
    let t = token("growth", tenant);
    let v = echoed(get("/v1/shipments/abc?status=pending&page=2", Some(&t)).await).await;
    assert_eq!(v["path"], "/order_intake/v1/shipments/abc");
    assert_eq!(v["query"], "status=pending&page=2");
    assert_eq!(v["headers"]["x-tenant-id"], json!([tenant.to_string()]));
    assert_eq!(
        v["headers"]["authorization"],
        json!([format!("Bearer {t}")]),
        "the bearer token is passed through so the service can validate it itself"
    );
}

#[tokio::test]
async fn a_client_cannot_choose_its_own_tenant_header() {
    // The CORS layer even lists x-tenant-id as an allowed request header, so a
    // browser client can send it. Upstreams must see exactly one value: the one
    // derived from the validated token.
    let tenant = Uuid::new_v4();
    let victim = Uuid::new_v4();
    let t = token("starter", tenant);
    let resp = client()
        .get(format!("{}/v1/shipments", h().gateway))
        .bearer_auth(&t)
        .header("X-Tenant-Id", victim.to_string())
        .send()
        .await
        .expect("send");
    let v = echoed(resp).await;
    assert_eq!(
        v["headers"]["x-tenant-id"],
        json!([tenant.to_string()]),
        "a client-supplied X-Tenant-Id must be dropped, not forwarded alongside the real one"
    );
}

#[tokio::test]
async fn method_and_body_are_forwarded() {
    let t = token("starter", Uuid::new_v4());
    for m in ["POST", "PUT", "PATCH", "DELETE"] {
        let resp = client()
            .request(m.parse().expect("method"), format!("{}/v1/orders", h().gateway))
            .bearer_auth(&t)
            .body("payload-123")
            .send()
            .await
            .expect("send");
        let v = echoed(resp).await;
        assert_eq!(v["method"], m);
        assert_eq!(v["body"], "payload-123");
    }
}

#[tokio::test]
async fn each_tier_prefix_reaches_the_service_that_owns_it() {
    let t = token("starter", Uuid::new_v4());
    for (path, upstream) in [
        ("/v1/users/me", "identity"),
        ("/v1/assignments/1/accept", "dispatch"),
        ("/v1/drivers/1/cancel-assignment", "dispatch"),
        ("/v1/drivers/1/status", "driver_ops"),
        ("/v1/pod/submit", "pod"),
        ("/v1/customers/1/invoices", "payments"),
        ("/v1/customers/1", "cdp"),
        ("/v1/omnideliv/orders", "omnideliv"),
        ("/mcp", "ai_layer"),
        ("/api/v1/compliance/x", "compliance"),
    ] {
        let v = echoed(get(path, Some(&t)).await).await;
        assert_eq!(v["path"], format!("/{upstream}{path}"), "{path}");
    }
}

#[tokio::test]
async fn unroutable_paths_are_404_after_auth() {
    let t = token("starter", Uuid::new_v4());
    for path in [
        "/v1/nope",
        "/v1/internal/auth/exchange-firebase",
        "/v1/field-ops/internal/couriers",
        "/v1/omnideliv/internal/catalog/ingest",
        // Optional upstreams that are not configured in this environment.
        "/v1/promotions/offers",
        "/v1/webhooks",
        "/v1/connectors/x",
    ] {
        let r = get(path, Some(&t)).await;
        assert_eq!(r.status(), 404, "{path}");
        let body: Value = r.json().await.expect("json");
        assert_eq!(body["error"], "No upstream service found for this path");
    }
    // ...and an unauthenticated caller is told 401, not 404, so the route
    // table is not an oracle.
    assert_eq!(get("/v1/nope", None).await.status(), 401);
}

#[tokio::test]
async fn upstream_status_and_headers_pass_through() {
    let t = token("starter", Uuid::new_v4());
    let r = get("/v1/shipments/status/418", Some(&t)).await;
    assert_eq!(r.status(), 418);
    assert_eq!(r.headers().get("x-upstream").and_then(|v| v.to_str().ok()), Some("yes"));
    assert_eq!(r.text().await.expect("body"), "upstream said so");
}

#[tokio::test]
async fn a_dead_upstream_is_502() {
    let t = token("starter", Uuid::new_v4());
    let r = get("/v1/field-ops/assignments/offer", Some(&t)).await;
    assert_eq!(r.status(), 502);
    let body: Value = r.json().await.expect("json");
    assert_eq!(body["error"], "Upstream service unavailable");
}

// ── rate-limit headers & Redis failure mode ─────────────────────────────

#[tokio::test]
async fn redis_down_fails_open_and_advertises_the_tier_limit() {
    for (tier, limit) in [("starter", "100"), ("growth", "500"), ("enterprise", "10000")] {
        let t = token(tier, Uuid::new_v4());
        let r = get("/v1/shipments", Some(&t)).await;
        assert_eq!(r.status(), 200, "{tier}: Redis outage must not take the API down");
        assert_eq!(
            r.headers().get("x-ratelimit-limit").and_then(|v| v.to_str().ok()),
            Some(limit),
            "{tier}"
        );
    }
}

// ── anonymous diner containment ─────────────────────────────────────────

#[tokio::test]
async fn a_diner_token_is_confined_to_the_dine_in_allowlist() {
    let tenant = Uuid::new_v4();
    let d = diner_token(tenant);

    for path in ["/v1/shipments", "/v1/users/me", "/v1/omnideliv/vendors/me", "/v1/omnideliv/mesh/run"] {
        let r = get(path, Some(&d)).await;
        assert_eq!(r.status(), 403, "{path} must be closed to a diner");
        let body: Value = r.json().await.expect("json");
        assert_eq!(body["error"], "This session cannot access that resource");
    }

    // The allowed shape reaches omnideliv with the diner's tenant.
    let v = echoed(get("/v1/omnideliv/catalog/search?q=tea", Some(&d)).await).await;
    assert_eq!(v["path"], "/omnideliv/v1/omnideliv/catalog/search");
    assert_eq!(v["headers"]["x-tenant-id"], json!([tenant.to_string()]));

    // Method is part of the rule.
    let r = client()
        .delete(format!("{}/v1/omnideliv/orders", h().gateway))
        .bearer_auth(&d)
        .send()
        .await
        .expect("send");
    assert_eq!(r.status(), 403);

    // A normal user token is not subject to the diner allowlist.
    let user = token("starter", tenant);
    assert_eq!(get("/v1/shipments", Some(&user)).await.status(), 200);
}

// ── X-Forwarded-For ────────────────────────────────────────────────────

#[tokio::test]
async fn the_peer_is_appended_to_x_forwarded_for_once() {
    let t = token("starter", Uuid::new_v4());

    let v = echoed(get("/v1/shipments", Some(&t)).await).await;
    assert_eq!(v["headers"]["x-forwarded-for"], json!(["127.0.0.1"]));

    let resp = client()
        .get(format!("{}/v1/shipments", h().gateway))
        .bearer_auth(&t)
        .header("X-Forwarded-For", "203.0.113.9")
        .send()
        .await
        .expect("send");
    let v = echoed(resp).await;
    assert_eq!(
        v["headers"]["x-forwarded-for"],
        json!(["203.0.113.9, 127.0.0.1"]),
        "an upstream hop's value is preserved and the peer appended, as a single header"
    );
}

// ── CORS ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn options_never_requires_auth() {
    // Plain OPTIONS with no token must be answered (by the CORS layer or the
    // handler's short-circuit), never 401.
    let r = client()
        .request(reqwest::Method::OPTIONS, format!("{}/v1/shipments", h().gateway))
        .send()
        .await
        .expect("send");
    assert!(r.status().is_success(), "got {}", r.status());

    // A real preflight is answered by the CORS layer.
    let r = client()
        .request(reqwest::Method::OPTIONS, format!("{}/v1/shipments", h().gateway))
        .header("Origin", "https://os.example")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "authorization,x-tenant-id")
        .send()
        .await
        .expect("send");
    assert!(r.status().is_success(), "preflight got {}", r.status());
    assert!(r.headers().contains_key("access-control-allow-origin"));
}

// ── gateway-owned endpoints ─────────────────────────────────────────────

#[tokio::test]
async fn health_reports_degraded_when_redis_is_unreachable() {
    let r = get("/health", None).await;
    assert_eq!(r.status(), 503);
    let body: Value = r.json().await.expect("json");
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["redis"], false);
}

#[tokio::test]
async fn the_mcp_registry_endpoints_list_the_seeded_platform_tools() {
    let v: Value = get("/mcp/tools", None).await.json().await.expect("json");
    let tools = v["tools"].as_array().expect("tools array");
    assert_eq!(v["count"], tools.len());
    assert!(
        tools.iter().any(|t| t["service"] == "dispatch" && t["name"] == "assign_driver"),
        "seeded dispatch tool missing"
    );

    let v: Value = get("/mcp/servers", None).await.json().await.expect("json");
    assert!(!v["servers"].as_array().expect("servers").is_empty());
}
