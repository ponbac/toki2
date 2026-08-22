use std::{
    env, fmt,
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes},
    extract::{MatchedPath, State},
    http::{header, HeaderMap, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_appender_tracing::layer::{OpenTelemetryTracingBridge, TracingSpanAttributes};
use opentelemetry_sdk::{
    logs::{SdkLogger, SdkLoggerProvider},
    metrics::{Aggregation, Instrument, InstrumentKind, SdkMeterProvider, Stream},
    trace::SdkTracerProvider,
    Resource,
};
use regex::Regex;
use serde_json::Value;
use tower_http::{
    classify::ServerErrorsFailureClass,
    trace::{OnFailure, TraceLayer},
};
use tracing::{field, Span};
use tracing_subscriber::{
    filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};
use url::form_urlencoded;

use crate::config::{DatabaseSettings, ObservabilitySettings, Settings};

pub(crate) mod metrics;
mod sql_spans;

const REDACTED: &str = "[REDACTED]";
type BoxError = Box<dyn std::error::Error + Send + Sync>;
const DURATION_HISTOGRAM_BOUNDARIES_SECONDS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0,
];

const SPAN_ATTRIBUTE_ALLOWLIST: &[&str] = &[
    "http.route",
    "http.request.method",
    "http.response.status_code",
    "user.id",
    "repo.key",
    "organization",
    "project",
    "repo_name",
    "project.id",
    "work_item_id",
    "push_subscription.id",
    "target_column_name",
    "next_present",
    "provider.name",
    "operation.name",
    "kleer.method",
    "kleer.route",
    "kleer.status_code",
    "trigger",
    "retry_attempt",
    "timeout_seconds",
    "pr_count",
    "changed_pr_count",
    "notification_count",
    "push_notification_count",
    "notification_error",
    "db.system.name",
    "db.namespace",
    "db.collection.name",
    "db.operation.name",
    "db.query.summary",
    "db.query.text",
    "server.address",
    "server.port",
    "db.name",
    "db.operation",
    "db.response.affected_rows",
    "db.response.returned_rows",
    "db.response.status_code",
    "db.sql.table",
    "error.message",
    "error.stacktrace",
    "error.type",
    "net.peer.name",
    "net.peer.port",
    "otel.status_code",
    "otel.status_description",
    "peer.service",
];

pub(crate) struct ObservabilityGuard {
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl ObservabilityGuard {
    pub fn shutdown(self) {
        if let Some(provider) = self.meter_provider {
            if let Err(error) = provider.shutdown() {
                eprintln!("failed to shutdown OTEL meter provider: {error}");
            }
        }

        if let Some(provider) = self.tracer_provider {
            if let Err(error) = provider.shutdown() {
                eprintln!("failed to shutdown OTEL tracer provider: {error}");
            }
        }

        if let Some(provider) = self.logger_provider {
            if let Err(error) = provider.shutdown() {
                eprintln!("failed to shutdown OTEL logger provider: {error}");
            }
        }
    }
}

pub(crate) fn init(settings: &Settings) -> Result<ObservabilityGuard, BoxError> {
    let env_filter = env_filter();
    let otel = build_otel_layers()?;
    let is_production = env::var("APP_ENVIRONMENT")
        .map(|value| value.eq_ignore_ascii_case("production"))
        .unwrap_or(!cfg!(debug_assertions));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(otel.trace_layer)
        .with(otel.log_layer);

    if is_production {
        subscriber
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        subscriber
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
    }

    tracing::debug!(
        capture_request_bodies = settings.observability.capture_request_bodies,
        request_body_max_logged_bytes = settings.observability.request_body_max_logged_bytes,
        request_body_max_buffered_bytes = settings.observability.request_body_max_buffered_bytes,
        otel_enabled = otel.tracer_provider.is_some()
            || otel.logger_provider.is_some()
            || otel.meter_provider.is_some(),
        otel_metrics_enabled = otel.meter_provider.is_some(),
        "Observability initialized"
    );

    Ok(ObservabilityGuard {
        tracer_provider: otel.tracer_provider,
        logger_provider: otel.logger_provider,
        meter_provider: otel.meter_provider,
    })
}

pub(crate) fn db_operation_span(
    database: &DatabaseSettings,
    operation_name: &str,
    statement: &str,
) -> Span {
    let summary = sql_spans::query_summary(statement);

    tracing::info_span!(
        "db.query",
        otel.name = %format!("{operation_name} {summary}"),
        otel.kind = "client",
        db.system.name = "postgresql",
        db.namespace = database.database_name.as_str(),
        db.operation.name = operation_name,
        db.query.summary = %summary,
        db.query.text = statement,
        server.address = database.host.as_str(),
        server.port = database.port,
    )
}

pub(crate) fn record_user_id(user_id: impl fmt::Display) {
    Span::current().record("user.id", field::display(user_id));
}

pub(crate) fn record_repo_key(repo_key: impl fmt::Display) {
    Span::current().record("repo.key", field::display(repo_key));
}

pub(crate) fn record_span_field(name: &'static str, value: impl fmt::Display) {
    Span::current().record(name, field::display(value));
}

fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy()
        .add_directive("hyper=warn".parse().unwrap())
        .add_directive("h2=warn".parse().unwrap())
        .add_directive("tonic=warn".parse().unwrap())
        .add_directive("opentelemetry=warn".parse().unwrap())
        .add_directive("azure_core::policies::transport=info".parse().unwrap())
        .add_directive("tower_sessions=warn".parse().unwrap())
        .add_directive("tower_sessions_core=warn".parse().unwrap())
        .add_directive("axum_login=warn".parse().unwrap())
}

struct OtelLayers<S>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    trace_layer:
        Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>,
    log_layer: Option<OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>>,
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

fn build_otel_layers<S>() -> Result<OtelLayers<S>, BoxError>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    let traces_endpoint_configured = otel_endpoint_configured("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
    let logs_endpoint_configured = otel_endpoint_configured("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT");
    let metrics_endpoint_configured =
        otel_endpoint_configured("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT");
    let sdk_disabled = otel_sdk_disabled();

    if (!traces_endpoint_configured && !logs_endpoint_configured && !metrics_endpoint_configured)
        || sdk_disabled
    {
        return Ok(OtelLayers {
            trace_layer: None,
            log_layer: None,
            tracer_provider: None,
            logger_provider: None,
            meter_provider: None,
        });
    }

    let resource = Resource::builder()
        .with_service_name(env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "toki-api".to_string()))
        .with_attributes(resource_attributes_from_env())
        .build();

    let (trace_layer, tracer_provider) = if traces_endpoint_configured {
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()?;
        let span_exporter = sql_spans::SqlSpanNamingExporter::new(span_exporter);
        let tracer_provider = SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(span_exporter)
            .build();
        let tracer = tracer_provider.tracer("toki-api");
        global::set_tracer_provider(tracer_provider.clone());
        let trace_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_location(false)
            .with_threads(false);
        (Some(trace_layer), Some(tracer_provider))
    } else {
        (None, None)
    };

    let (log_layer, logger_provider) = if logs_endpoint_configured {
        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .build()?;
        let logger_provider = SdkLoggerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(log_exporter)
            .build();
        let log_layer = OpenTelemetryTracingBridge::builder(&logger_provider)
            .with_tracing_span_attributes(TracingSpanAttributes::allowlist(
                SPAN_ATTRIBUTE_ALLOWLIST.iter().copied(),
            ))
            .build();
        (Some(log_layer), Some(logger_provider))
    } else {
        (None, None)
    };

    let meter_provider = if metrics_endpoint_configured {
        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .build()?;
        let meter_provider = SdkMeterProvider::builder()
            .with_resource(resource)
            .with_view(duration_histogram_view)
            .with_periodic_exporter(metric_exporter)
            .build();
        global::set_meter_provider(meter_provider.clone());
        metrics::init();
        Some(meter_provider)
    } else {
        None
    };

    Ok(OtelLayers {
        trace_layer,
        log_layer,
        tracer_provider,
        logger_provider,
        meter_provider,
    })
}

fn duration_histogram_view(instrument: &Instrument) -> Option<Stream> {
    if instrument.kind() == InstrumentKind::Histogram
        && instrument.unit() == "s"
        && instrument.name().ends_with(".duration")
    {
        Stream::builder()
            .with_aggregation(Aggregation::ExplicitBucketHistogram {
                boundaries: DURATION_HISTOGRAM_BOUNDARIES_SECONDS.to_vec(),
                record_min_max: true,
            })
            .build()
            .ok()
    } else {
        None
    }
}

fn otel_endpoint_configured(signal_endpoint_var: &str) -> bool {
    otel_endpoint_configured_from(signal_endpoint_var, |name| env::var(name).ok())
}

fn otel_endpoint_configured_from<F>(signal_endpoint_var: &str, mut lookup: F) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    ["OTEL_EXPORTER_OTLP_ENDPOINT", signal_endpoint_var]
        .into_iter()
        .any(|name| lookup(name).is_some_and(|value| !value.trim().is_empty()))
}

fn otel_sdk_disabled() -> bool {
    otel_sdk_disabled_from(|name| env::var(name).ok())
}

fn otel_sdk_disabled_from<F>(mut lookup: F) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    lookup("OTEL_SDK_DISABLED")
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn resource_attributes_from_env() -> Vec<KeyValue> {
    env::var("OTEL_RESOURCE_ATTRIBUTES")
        .ok()
        .map(|attributes| {
            attributes
                .split(',')
                .filter_map(|pair| pair.split_once('='))
                .filter_map(|(key, value)| {
                    let key = key.trim();
                    if key.is_empty() || key == "service.name" {
                        None
                    } else {
                        Some(KeyValue::new(key.to_string(), value.trim().to_string()))
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

type HttpTraceLayer = TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    fn(&Request<Body>) -> Span,
    (),
    (),
    (),
    (),
    TraceOnFailure,
>;

pub(crate) fn http_trace_layer() -> HttpTraceLayer {
    TraceLayer::new_for_http()
        .make_span_with(make_http_span as fn(&Request<Body>) -> Span)
        .on_request(())
        .on_response(())
        .on_body_chunk(())
        .on_eos(())
        .on_failure(TraceOnFailure)
}

fn make_http_span(request: &Request<Body>) -> Span {
    if request.method() == Method::OPTIONS {
        return Span::none();
    }

    let route = route_template(request);
    let query = request.uri().query().map(sanitize_query);
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    tracing::info_span!(
        "http.request",
        otel.name = %format!("HTTP {} {}", request.method(), route),
        otel.kind = "server",
        http.request.method = %request.method(),
        http.route = %route,
        url.path = %request.uri().path(),
        url.query = query.as_deref().unwrap_or_default(),
        user_agent.original = user_agent,
        http.response.status_code = field::Empty,
        latency_ms = field::Empty,
        user.id = field::Empty,
        repo.key = field::Empty,
        organization = field::Empty,
        project = field::Empty,
        repo_name = field::Empty,
        project.id = field::Empty,
        work_item_id = field::Empty,
        push_subscription.id = field::Empty,
        target_column_name = field::Empty,
        next_present = field::Empty,
    )
}

fn route_template(request: &Request<Body>) -> String {
    route_template_from_parts(
        request
            .extensions()
            .get::<MatchedPath>()
            .map(MatchedPath::as_str),
        request.uri().path(),
    )
}

fn metrics_route_template(request: &Request<Body>) -> String {
    request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn route_template_from_parts(matched_path: Option<&str>, path: &str) -> String {
    matched_path
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TraceOnFailure;

impl OnFailure<ServerErrorsFailureClass> for TraceOnFailure {
    fn on_failure(
        &mut self,
        failure_class: ServerErrorsFailureClass,
        latency: Duration,
        span: &Span,
    ) {
        if span.is_none() {
            return;
        }

        span.record("latency_ms", latency.as_secs_f64() * 1000.0);
        tracing::warn!(
            error = %failure_class,
            latency_ms = latency.as_secs_f64() * 1000.0,
            "HTTP request failed"
        );
    }
}

pub(crate) async fn log_http_response_middleware(request: Request<Body>, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let route = route_template(&request);
    let metrics_route = metrics_route_template(&request);
    let started_at = Instant::now();
    metrics::record_http_active_request_delta(method.as_str(), &metrics_route, 1);
    let response = next.run(request).await;
    let elapsed = started_at.elapsed();
    metrics::record_http_active_request_delta(method.as_str(), &metrics_route, -1);
    let latency_ms = elapsed.as_secs_f64() * 1000.0;
    let status = response.status().as_u16();
    metrics::record_http_request(method.as_str(), &metrics_route, status, elapsed);

    let span = Span::current();
    span.record("http.response.status_code", status);
    span.record("latency_ms", latency_ms);

    tracing::info!(
        http.request.method = %method,
        http.route = %route,
        http.response.status_code = status,
        latency_ms,
        "{}",
        http_response_message(&method, &route, status, latency_ms)
    );

    response
}

fn http_response_message(method: &Method, route: &str, status: u16, latency_ms: f64) -> String {
    format!("HTTP {method} {route} responded {status} in {latency_ms:.1}ms")
}

pub(crate) async fn capture_request_body_middleware(
    State(settings): State<ObservabilitySettings>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();

    if parts.method == Method::OPTIONS
        || !settings.capture_request_bodies
        || !should_capture_body(&parts.headers, &settings)
    {
        return next.run(Request::from_parts(parts, body)).await;
    }

    let content_type = content_type(&parts.headers).unwrap_or_default().to_string();
    let collected = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            tracing::warn!(error = %error, "Failed to capture request body");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    log_request_body(
        &collected,
        &content_type,
        settings.request_body_max_logged_bytes,
    );
    next.run(Request::from_parts(parts, Body::from(collected)))
        .await
}

fn should_capture_body(headers: &HeaderMap, settings: &ObservabilitySettings) -> bool {
    let Some(content_length) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return false;
    };

    content_length <= settings.request_body_max_buffered_bytes
        && content_type(headers).is_some_and(is_text_like_content_type)
}

fn content_type(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim())
}

fn is_text_like_content_type(content_type: &str) -> bool {
    let content_type = content_type.to_ascii_lowercase();
    content_type == "application/json"
        || content_type.ends_with("+json")
        || content_type == "application/x-www-form-urlencoded"
        || content_type.starts_with("text/")
}

fn log_request_body(body: &Bytes, content_type: &str, max_logged_bytes: usize) {
    let body_text = String::from_utf8_lossy(body);
    let redacted = redact_body(&body_text, content_type);
    let (content, truncated) = truncate_to_bytes(&redacted, max_logged_bytes);

    tracing::info!(
        name = "http.request.body",
        http.request.body.content = content,
        http.request.body.truncated = truncated,
        http.request.body.bytes = body.len(),
        http.request.body.content_type = content_type,
        "http.request.body"
    );
}

fn redact_body(body: &str, content_type: &str) -> String {
    let content_type = content_type.to_ascii_lowercase();
    if content_type == "application/json" || content_type.ends_with("+json") {
        return redact_json_body(body).unwrap_or_else(|| redact_text(body));
    }

    if content_type == "application/x-www-form-urlencoded" {
        return redact_form_or_query(body);
    }

    redact_text(body)
}

fn redact_json_body(body: &str) -> Option<String> {
    let mut value = serde_json::from_str::<Value>(body).ok()?;
    redact_json_value(&mut value);
    serde_json::to_string(&value).ok()
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_sensitive_key(key) {
                    *value = Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_json_value),
        _ => {}
    }
}

fn redact_form_or_query(body: &str) -> String {
    let pairs = form_urlencoded::parse(body.as_bytes());
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        let value = if is_sensitive_key(&key) {
            REDACTED.into()
        } else {
            value
        };
        serializer.append_pair(&key, &value);
    }
    serializer.finish()
}

fn redact_text(text: &str) -> String {
    static TOKEN_ASSIGNMENT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(
            r#"(?ix)
            (?P<key>token|access_token|refresh_token|id_token|secret|client_secret|password|api_key|apikey|authorization|cookie|set_cookie|auth|p256dh|endpoint|code|state|pat)
            (?P<sep>\s*[:=]\s*)
            (?P<value>"[^"]*"|'[^']*'|[^\s,"'&;]+)
            "#,
        )
        .expect("valid redaction regex")
    });

    TOKEN_ASSIGNMENT
        .replace_all(text, |captures: &regex::Captures| {
            let quote = captures["value"]
                .chars()
                .next()
                .filter(|value| *value == '"' || *value == '\'')
                .map(|value| value.to_string())
                .unwrap_or_default();
            format!(
                "{}{}{}{}{}",
                &captures["key"], &captures["sep"], quote, REDACTED, quote
            )
        })
        .to_string()
}

fn sanitize_query(query: &str) -> String {
    redact_form_or_query(query)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    let normalized = normalized.trim_matches('_');
    matches!(
        normalized,
        "token"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "secret"
            | "client_secret"
            | "password"
            | "api_key"
            | "apikey"
            | "authorization"
            | "cookie"
            | "set_cookie"
            | "auth"
            | "p256dh"
            | "endpoint"
            | "code"
            | "state"
            | "pat"
    ) || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
}

fn truncate_to_bytes(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }

    if max_bytes == 0 {
        return (String::new(), true);
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
        middleware,
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use serde::Deserialize;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn observability_json_redaction_handles_nested_objects_and_arrays() {
        let body = json!({
            "accessToken": "abc",
            "nested": {
                "client_secret": "def",
                "items": [
                    {"password": "pw"},
                    {"name": "safe"}
                ]
            }
        })
        .to_string();

        let redacted = redact_body(&body, "application/json");
        assert!(redacted.contains(REDACTED));
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("def"));
        assert!(!redacted.contains("pw"));
        assert!(redacted.contains("safe"));
    }

    #[test]
    fn observability_form_and_query_redaction_redacts_sensitive_keys() {
        let redacted = redact_form_or_query("code=abc&state=def&name=visible&api_key=ghi");

        assert!(redacted.contains("name=visible"));
        assert!(redacted.contains("code=%5BREDACTED%5D"));
        assert!(redacted.contains("state=%5BREDACTED%5D"));
        assert!(redacted.contains("api_key=%5BREDACTED%5D"));
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("def"));
        assert!(!redacted.contains("ghi"));
    }

    #[test]
    fn observability_text_redaction_fallback_redacts_token_like_assignments() {
        let redacted = redact_text(r#"token="abc" password=secret normal=value"#);

        assert_eq!(
            redacted,
            r#"token="[REDACTED]" password=[REDACTED] normal=value"#
        );
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("secret"));
    }

    #[test]
    fn observability_truncation_respects_utf8_boundaries() {
        let (truncated, was_truncated) = truncate_to_bytes("aåb", 2);

        assert_eq!(truncated, "a");
        assert!(was_truncated);
    }

    #[test]
    fn observability_otel_endpoint_detection_honors_signal_specific_vars() {
        let traces_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT" => Some("http://collector:4317".to_string()),
                _ => None,
            },
        );
        let logs_configured =
            otel_endpoint_configured_from("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", |name| match name {
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT" => Some(" http://collector:4317 ".to_string()),
                _ => None,
            });
        let generic_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://collector:4317".to_string()),
                _ => None,
            },
        );
        let blank_value_is_ignored = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_ENDPOINT" => Some(" ".to_string()),
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT" => Some("\t".to_string()),
                _ => None,
            },
        );

        assert!(traces_configured);
        assert!(logs_configured);
        assert!(generic_configured);
        assert!(!blank_value_is_ignored);
    }

    #[test]
    fn observability_metrics_endpoint_detection_honors_metrics_specific_var() {
        let metrics_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT" => Some("http://collector:4317".to_string()),
                _ => None,
            },
        );

        assert!(metrics_configured);
    }

    #[test]
    fn observability_generic_endpoint_enables_metrics() {
        let metrics_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://collector:4317".to_string()),
                _ => None,
            },
        );

        assert!(metrics_configured);
    }

    #[test]
    fn observability_blank_metrics_endpoint_is_ignored() {
        let metrics_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT" => Some(" ".to_string()),
                _ => None,
            },
        );

        assert!(!metrics_configured);
    }

    #[test]
    fn observability_sdk_disabled_disables_all_providers() {
        let metrics_endpoint_configured = otel_endpoint_configured_from(
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            |name| match name {
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT" => Some("http://collector:4317".to_string()),
                _ => None,
            },
        );
        let sdk_disabled = otel_sdk_disabled_from(|name| match name {
            "OTEL_SDK_DISABLED" => Some("true".to_string()),
            _ => None,
        });

        assert!(metrics_endpoint_configured);
        assert!(!(metrics_endpoint_configured && !sdk_disabled));
        assert!(otel_sdk_disabled_from(|name| match name {
            "OTEL_SDK_DISABLED" => Some("true".to_string()),
            _ => None,
        }));
        assert!(otel_sdk_disabled_from(|name| match name {
            "OTEL_SDK_DISABLED" => Some(" TRUE ".to_string()),
            _ => None,
        }));
        assert!(!otel_sdk_disabled_from(|name| match name {
            "OTEL_SDK_DISABLED" => Some("false".to_string()),
            _ => None,
        }));
    }

    #[test]
    fn observability_binary_and_multipart_bodies_are_skipped() {
        let settings = ObservabilitySettings {
            capture_request_bodies: true,
            request_body_max_logged_bytes: 100,
            request_body_max_buffered_bytes: 100,
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "10".parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "multipart/form-data".parse().unwrap());
        assert!(!should_capture_body(&headers, &settings));

        headers.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        assert!(!should_capture_body(&headers, &settings));
    }

    #[tokio::test]
    async fn observability_middleware_reconstructs_body_for_json_handlers() {
        #[derive(Deserialize)]
        struct Payload {
            name: String,
        }

        async fn handler(Json(payload): Json<Payload>) -> impl IntoResponse {
            payload.name
        }

        let settings = ObservabilitySettings {
            capture_request_bodies: true,
            request_body_max_logged_bytes: 100,
            request_body_max_buffered_bytes: 100,
        };
        let app = Router::new()
            .route("/", post(handler))
            .layer(middleware::from_fn_with_state(
                settings,
                capture_request_body_middleware,
            ));
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONTENT_LENGTH, "15")
            .body(Body::from(r#"{"name":"Ada"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"Ada");
    }

    #[tokio::test]
    async fn observability_middleware_returns_bad_request_when_body_capture_fails() {
        async fn handler() -> impl IntoResponse {
            StatusCode::NO_CONTENT
        }

        let settings = ObservabilitySettings {
            capture_request_bodies: true,
            request_body_max_logged_bytes: 100,
            request_body_max_buffered_bytes: 100,
        };
        let app = Router::new()
            .route("/", post(handler))
            .layer(middleware::from_fn_with_state(
                settings,
                capture_request_body_middleware,
            ));
        let body = Body::from_stream(futures_util::stream::once(async {
            Err::<Bytes, std::io::Error>(std::io::Error::other("body read failed"))
        }));
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONTENT_LENGTH, "15")
            .body(body)
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn observability_oversized_content_length_skips_capture() {
        let settings = ObservabilitySettings {
            capture_request_bodies: true,
            request_body_max_logged_bytes: 100,
            request_body_max_buffered_bytes: 10,
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "11".parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

        assert!(!should_capture_body(&headers, &settings));
    }

    #[test]
    fn observability_http_trace_span_uses_route_template() {
        let request = Request::builder()
            .uri("/repositories/123?code=secret&state=sensitive")
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            route_template_from_parts(Some("/repositories/:id"), request.uri().path()),
            "/repositories/:id"
        );
        assert_eq!(
            sanitize_query(request.uri().query().unwrap()),
            "code=%5BREDACTED%5D&state=%5BREDACTED%5D"
        );
    }

    #[test]
    fn observability_http_trace_skips_options() {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/time-tracking/time-entries")
            .body(Body::empty())
            .unwrap();

        assert!(make_http_span(&request).is_none());
    }

    #[test]
    fn observability_http_response_message_includes_request_shape() {
        assert_eq!(
            http_response_message(&Method::GET, "/pull-requests/list", 200, 40.92074),
            "HTTP GET /pull-requests/list responded 200 in 40.9ms"
        );
    }

    #[test]
    fn observability_auth_callback_query_redaction_does_not_expose_code() {
        let redacted = sanitize_query("code=oauth-code&state=csrf&next=/prs");

        assert!(!redacted.contains("oauth-code"));
        assert!(!redacted.contains("csrf"));
        assert!(redacted.contains("next=%2Fprs"));
    }
}
