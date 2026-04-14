use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde_json::json;
use tower_http::trace::TraceLayer;

use logisticos_auth::jwt::JwtService;

use crate::{
    config::Config,
    proxy::ProxyClient,
    ratelimit::{check_rate_limit, limit_for_tier},
    registry::{McpRegistry, seed_platform_servers},
};

#[derive(Clone)]
pub struct AppState {
    pub proxy:    Arc<ProxyClient>,
    pub redis:    Arc<redis::Client>,
    pub jwt:      Arc<JwtService>,
    pub registry: Arc<McpRegistry>,
    pub cfg:      Arc<Config>,
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "api-gateway",
        env: &cfg.app.env,
        otlp_endpoint: None,
        log_level: None,
    })?;

    let redis_client = redis::Client::open(cfg.redis.url.clone())?;
    let proxy_client = Arc::new(ProxyClient::new(cfg.services.clone()));
    let jwt_service = Arc::new(JwtService::new(
        &cfg.auth.jwt_secret,
        cfg.auth.jwt_expiry_seconds,
        cfg.auth.refresh_token_expiry_seconds,
    ));

    let mut registry = McpRegistry::new();
    seed_platform_servers(&mut registry, &cfg.services);

    let state = AppState {
        proxy:    proxy_client,
        redis:    Arc::new(redis_client),
        jwt:      jwt_service,
        registry: Arc::new(registry),
        cfg:      Arc::new(cfg.clone()),
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/mcp/servers", get(list_mcp_servers))
        .route("/mcp/tools", get(list_mcp_tools))
        .fallback(axum::routing::any(proxy_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "api-gateway listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Proxy handler — JWT auth + rate limiting + forwarding
// ---------------------------------------------------------------------------

async fn proxy_handler(State(state): State<AppState>, req: Request<Body>) -> Response {
    // 1. Extract JWT from Authorization: Bearer <token>
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);

    // Public paths that skip authentication entirely.
    let path = req.uri().path();
    let is_public = path.starts_with("/v1/tracking/public/")
        || path == "/v1/auth/login"
        || path == "/v1/auth/register"
        || path == "/v1/auth/refresh"
        || path == "/v1/auth/signup"
        || path == "/v1/auth/otp/send"
        || path == "/v1/auth/otp/verify"
        || path == "/v1/auth/forgot-password"
        || path == "/v1/auth/reset-password";

    let (tenant_id, subscription_tier) = if is_public {
        (String::from("public"), String::from("starter"))
    } else {
        let token = match auth_header {
            Some(t) => t,
            None => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Missing Authorization header"})),
                )
                    .into_response();
            }
        };
        match state.jwt.validate_access_token(&token) {
            Ok(token_data) => {
                let claims = token_data.claims;
                (claims.tenant_id.to_string(), claims.subscription_tier.clone())
            },
            Err(_) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Invalid or expired token"})),
                )
                    .into_response();
            }
        }
    };

    // 2. Rate limiting via Redis.
    let rate_result = async {
        let mut conn = state.redis.get_multiplexed_async_connection().await?;
        check_rate_limit(&mut conn, &tenant_id, &subscription_tier).await
    }
    .await;

    let (allowed, remaining, reset_in) = match rate_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(err = %e, "Rate limit Redis error — allowing request");
            (true, 0, 60)
        }
    };

    if !allowed {
        let mut resp = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "Rate limit exceeded",
                "retry_after_seconds": reset_in
            })),
        )
            .into_response();
        insert_rate_limit_headers(resp.headers_mut(), 0, remaining, reset_in, &subscription_tier);
        return resp;
    }

    // 3. Resolve upstream.
    let upstream_base = match state.proxy.resolve_upstream(path) {
        Some(u) => u.to_owned(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "No upstream service found for this path"})),
            )
                .into_response();
        }
    };

    // 4. Forward request to upstream service.
    let upstream_url = build_upstream_url(&upstream_base, req.uri());
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = match axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Failed to read request body"}))).into_response();
        }
    };

    let mut upstream_req: reqwest::RequestBuilder = state
        .proxy
        .client
        .request(map_method(&method), &upstream_url)
        .body(body_bytes);

    // Forward safe headers; inject X-Tenant-Id for downstream services.
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            upstream_req = upstream_req.header(name.as_str(), v);
        }
    }
    upstream_req = upstream_req.header("X-Tenant-Id", &tenant_id);

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(err = %e, upstream = %upstream_url, "Upstream request failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "Upstream service unavailable"})),
            )
                .into_response();
        }
    };

    // 5. Stream upstream response back to client.
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let upstream_headers = upstream_resp.headers().clone();
    let resp_bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({"error": "Failed to read upstream response"}))).into_response();
        }
    };

    let mut response = Response::builder().status(status);
    let resp_headers = response.headers_mut().unwrap();
    for (name, value) in upstream_headers.iter() {
        if let Ok(name_str) = std::str::from_utf8(name.as_str().as_bytes()) {
            if !is_hop_by_hop_str(name_str) {
                if let Ok(header_name) = axum::http::HeaderName::from_bytes(name_str.as_bytes()) {
                    if let Ok(header_value) = axum::http::HeaderValue::from_bytes(value.as_bytes()) {
                        resp_headers.insert(header_name, header_value);
                    }
                }
            }
        }
    }
    insert_rate_limit_headers(
        resp_headers,
        limit_for_tier(&subscription_tier),
        remaining,
        reset_in,
        &subscription_tier,
    );

    response.body(Body::from(resp_bytes)).unwrap_or_else(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Response build error"}))).into_response()
    })
}

// ---------------------------------------------------------------------------
// MCP registry endpoints
// ---------------------------------------------------------------------------

async fn list_mcp_servers(State(state): State<AppState>) -> impl IntoResponse {
    let servers: Vec<_> = state
        .registry
        .all_tools()
        .into_iter()
        .map(|(svc, tool)| json!({"service": svc, "tool": tool.name, "description": tool.description}))
        .collect();
    Json(json!({"servers": servers}))
}

async fn list_mcp_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools: Vec<_> = state
        .registry
        .all_tools()
        .into_iter()
        .map(|(svc, tool)| json!({
            "service": svc,
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.input_schema,
        }))
        .collect();
    Json(json!({"tools": tools, "count": tools.len()}))
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let redis_ok = state
        .redis
        .get_multiplexed_async_connection()
        .await
        .map(|_| true)
        .unwrap_or(false);
    let status = if redis_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({"status": if redis_ok {"ok"} else {"degraded"}, "redis": redis_ok})))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_upstream_url(base: &str, uri: &axum::http::Uri) -> String {
    let base = base.trim_end_matches('/');
    let path = uri.path();
    match uri.query() {
        Some(q) => format!("{}{}?{}", base, path, q),
        None => format!("{}{}", base, path),
    }
}

fn map_method(m: &Method) -> reqwest::Method {
    reqwest::Method::from_bytes(m.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET)
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    is_hop_by_hop_str(name.as_str())
}

fn is_hop_by_hop_str(name: &str) -> bool {
    matches!(
        name,
        "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization"
        | "te" | "trailers" | "transfer-encoding" | "upgrade"
    )
}

fn insert_rate_limit_headers(
    headers: &mut HeaderMap,
    limit: u64,
    remaining: u64,
    reset_in: u64,
    _tier: &str,
) {
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert("X-RateLimit-Limit", v);
    }
    if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert("X-RateLimit-Remaining", v);
    }
    if let Ok(v) = HeaderValue::from_str(&reset_in.to_string()) {
        headers.insert("X-RateLimit-Reset", v);
    }
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
