//! Traces and Prometheus metrics.
//!
//! Traces go to an OTLP collector when `OTEL_EXPORTER_OTLP_ENDPOINT` is set and
//! stay on stdout otherwise, so a local run needs no collector. Metrics are
//! rendered on a separate listener (`ASTER_METRICS_BIND`, default
//! `0.0.0.0:9090`) because Prometheus cannot carry a session cookie and an
//! unauthenticated endpoint does not belong on the public router; the port is
//! gated by NetworkPolicy instead.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{linear_buckets, Histogram};
use prometheus_client::registry::Registry;
use prometheus_client_derive_encode::EncodeLabelSet;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::AppState;

/// Bounded label set: `route` is the matched axum path, never the raw URI, so a
/// scanner cannot blow up the series count.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RequestLabels {
    pub method: String,
    pub route: String,
    pub status: u16,
}

/// Process-wide request counters and latency.
pub struct Metrics {
    registry: Registry,
    requests: Family<RequestLabels, Counter>,
    latency: Family<RequestLabels, Histogram>,
}

impl Metrics {
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let requests = Family::<RequestLabels, Counter>::default();
        let latency = Family::<RequestLabels, Histogram>::new_with_constructor(|| {
            // 5ms buckets up to 100ms, then the +Inf bucket.
            Histogram::new(linear_buckets(0.005, 0.005, 20))
        });
        registry.register(
            "aster_http_requests",
            "HTTP requests by method, route and status",
            requests.clone(),
        );
        registry.register(
            "aster_http_request_seconds",
            "HTTP request latency in seconds",
            latency.clone(),
        );
        Self {
            registry,
            requests,
            latency,
        }
    }

    fn observe(&self, labels: &RequestLabels, seconds: f64) {
        self.requests.get_or_create(labels).inc();
        self.latency.get_or_create(labels).observe(seconds);
    }

    /// OpenMetrics text, which Prometheus scrapes as-is.
    pub fn encode(&self) -> String {
        let mut buffer = String::new();
        match encode(&mut buffer, &self.registry) {
            Ok(()) => buffer,
            Err(error) => {
                tracing::error!(%error, "cannot encode metrics");
                String::new()
            }
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Span for the HTTP server. `TraceLayer`'s default is a DEBUG span named
/// `request` with no semantic-convention fields, so under an `info` filter it
/// never reached the OTLP pipeline; this one is INFO and carries the attributes
/// collectors and Jaeger expect.
pub fn http_span<B>(request: &axum::http::Request<B>) -> tracing::Span {
    tracing::info_span!(
        "http.server",
        otel.kind = "server",
        otel.name = %format!("{} {}", request.method(), request.uri().path()),
        http.request.method = %request.method(),
        url.path = %request.uri().path(),
    )
}

/// Records every request the public router answers.
pub async fn record(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let labels = RequestLabels {
        method: request.method().to_string(),
        route: request
            .extensions()
            .get::<MatchedPath>()
            .map(|path| path.as_str().to_string())
            .unwrap_or_else(|| "unmatched".to_string()),
        status: 0,
    };
    let response = next.run(request).await;
    state.metrics.observe(
        &RequestLabels {
            status: response.status().as_u16(),
            ..labels
        },
        started.elapsed().as_secs_f64(),
    );
    response
}

/// The metrics listener: scrape endpoint plus a liveness probe.
pub fn metrics_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}

async fn metrics(State(state): State<Arc<AppState>>) -> Response {
    (
        [(
            CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        state.metrics.encode(),
    )
        .into_response()
}

/// Owns the tracer provider so it can be flushed on shutdown.
pub struct Telemetry {
    provider: Option<SdkTracerProvider>,
}

impl Telemetry {
    /// Installs the subscriber: stdout always, OTLP when an endpoint is set.
    pub fn init(service_name: &str) -> Self {
        let provider = otlp_provider(service_name);
        let fmt_layer = tracing_subscriber::fmt::layer();
        match provider
            .as_ref()
            .map(|provider| provider.tracer(service_name.to_string()))
        {
            Some(tracer) => {
                tracing_subscriber::registry()
                    .with(filter())
                    .with(fmt_layer)
                    .with(tracing_opentelemetry::layer().with_tracer(tracer))
                    .init();
            }
            None => {
                tracing_subscriber::registry()
                    .with(filter())
                    .with(fmt_layer)
                    .init();
            }
        }
        Self { provider }
    }

    /// Flushes buffered spans. Called before the process exits.
    pub fn shutdown(&self) {
        if let Some(provider) = &self.provider {
            if let Err(error) = provider.shutdown() {
                tracing::warn!(%error, "tracer provider shutdown");
            }
        }
    }
}

fn filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=info".into())
}

/// `None` without `OTEL_EXPORTER_OTLP_ENDPOINT`: tracing then stops at stdout.
fn otlp_provider(service_name: &str) -> Option<SdkTracerProvider> {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|endpoint| !endpoint.trim().is_empty())?;
    let exporter = match SpanExporter::builder().with_http().build() {
        Ok(exporter) => exporter,
        Err(error) => {
            // No subscriber is installed yet, so a log line would be dropped.
            eprintln!("OTLP exporter disabled: {error}");
            return None;
        }
    };
    Some(
        SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(sampler())
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name.to_string())
                    .with_attributes([KeyValue::new("service.version", env!("CARGO_PKG_VERSION"))])
                    .build(),
            )
            .build(),
    )
}

/// `OTEL_TRACES_SAMPLER` is not read by the SDK, so parse it here.
fn sampler() -> Sampler {
    let name =
        std::env::var("OTEL_TRACES_SAMPLER").unwrap_or_else(|_| "parentbased_always_on".into());
    let ratio = || {
        std::env::var("OTEL_TRACES_SAMPLER_ARG")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1.0)
    };
    match name.as_str() {
        "always_on" => Sampler::AlwaysOn,
        "always_off" => Sampler::AlwaysOff,
        "traceidratio" => Sampler::TraceIdRatioBased(ratio()),
        "parentbased_traceidratio" => {
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio())))
        }
        "parentbased_always_off" => Sampler::ParentBased(Box::new(Sampler::AlwaysOff)),
        "parentbased_always_on" => Sampler::ParentBased(Box::new(Sampler::AlwaysOn)),
        other => {
            tracing::warn!(
                sampler = other,
                "unknown OTEL_TRACES_SAMPLER, using always_on"
            );
            Sampler::ParentBased(Box::new(Sampler::AlwaysOn))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_requests_and_latency() {
        let metrics = Metrics::new();
        metrics.observe(
            &RequestLabels {
                method: "GET".into(),
                route: "/healthz".into(),
                status: 200,
            },
            0.01,
        );
        let text = metrics.encode();
        assert!(text.contains("aster_http_requests_total"), "{text}");
        assert!(text.contains("aster_http_request_seconds"), "{text}");
        assert!(text.contains("route=\"/healthz\""), "{text}");
    }
}
