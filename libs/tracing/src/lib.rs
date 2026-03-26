//! Tracing initialiser for all LogisticOS services.
//!
//! Call `init()` once at service startup before any other work.
//! Configures:
//!   - JSON structured logging (production) or pretty-print (development)
//!   - OpenTelemetry OTLP trace export (Jaeger / Grafana Tempo)
//!   - Automatic span enrichment: service_name, tenant_id, request_id

use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{propagation::TraceContextPropagator, runtime, trace as sdktrace, Resource};
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct TracingConfig<'a> {
    pub service_name: &'a str,
    pub env: &'a str,                      // "development" | "staging" | "production"
    pub otlp_endpoint: Option<&'a str>,    // e.g. "http://localhost:4317"
    pub log_level: Option<&'a str>,        // defaults to "info"
}

/// Initialize tracing for a service. Call once at startup.
pub fn init(cfg: TracingConfig<'_>) -> anyhow::Result<()> {
    let level = cfg.log_level.unwrap_or("info");
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    // ── OpenTelemetry OTLP pipeline ──────────────────────────
    if let Some(endpoint) = cfg.otlp_endpoint {
        global::set_text_map_propagator(TraceContextPropagator::new());

        let resource = Resource::new(vec![
            KeyValue::new("service.name", cfg.service_name.to_owned()),
            KeyValue::new("deployment.environment", cfg.env.to_owned()),
        ]);

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
            )
            .with_trace_config(sdktrace::Config::default().with_resource(resource))
            .install_batch(runtime::Tokio)?;

        if cfg.env == "development" {
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty())
                .with(otel_layer)
                .init();
        } else {
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json())
                .with(otel_layer)
                .init();
        }
    } else {
        // Local dev without OTLP — just structured logging
        if cfg.env == "development" {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty())
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json())
                .init();
        }
    }

    Ok(())
}

/// Emit a structured audit event at INFO level.
/// All mutations that touch tenant data must call this.
#[macro_export]
macro_rules! audit {
    ($action:expr, tenant = $tenant:expr, actor = $actor:expr, $($field:tt)*) => {
        tracing::info!(
            audit = true,
            action = $action,
            tenant_id = %$tenant,
            actor_id = %$actor,
            $($field)*
        );
    };
}
