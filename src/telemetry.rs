use std::{fmt::Display, net::SocketAddr};

use opentelemetry::{metrics::MeterProvider, sdk, KeyValue};
use prometheus::Registry;
use teloxide::{types::InlineQuery, utils::command::BotCommands};

pub fn setup_telemetry(registry: Registry) -> anyhow::Result<sdk::metrics::MeterProvider> {
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()?;

    let provider = sdk::metrics::MeterProvider::builder()
        .with_reader(exporter)
        .build();

    Ok(provider)
}

pub fn command_telemetry<T: Display + BotCommands>(
    provider: &sdk::metrics::MeterProvider,
) -> impl Fn(T) {
    let histo = provider.meter("commands");

    let command_metric = histo.u64_counter("command_name").init();
    move |issued_command| {
        let name = issued_command.to_string();
        let attributes = [KeyValue::new("command", name)];
        command_metric.add(1, &attributes)
    }
}

pub fn inline_telemetry(provider: &sdk::metrics::MeterProvider) -> impl Fn(InlineQuery) {
    let meter = provider.meter("inline");

    let length_metric = meter.u64_counter("inline_length").init();
    move |query: InlineQuery| {
        let attributes = [KeyValue::new("length", query.query.len() as i64)];
        length_metric.add(1, &attributes);
    }
}

pub fn update_telemetry(provider: &sdk::metrics::MeterProvider) -> impl Fn() {
    let meter = provider.meter("updates");
    let meter_metric = meter.u64_counter("updates").init();

    move || {
        meter_metric.add(1, &[]);
    }
}

async fn prometheus_serve(
    registry: Registry,
) -> Result<hyper::Response<hyper::Body>, hyper::Error> {
    let encoder = prometheus::TextEncoder::new();
    let encoded = encoder.encode_to_string(&registry.gather()).unwrap();

    let response = hyper::Response::builder()
        .status(200)
        .body(hyper::Body::from(encoded))
        .unwrap();

    Ok(response)
}

/// prepares and runs an http server on `address`
/// that responds with prometheus-encoded telemetry data
pub fn listen_prometheus_server(address: impl Into<SocketAddr>, registry: Registry) {
    use hyper::service::*;
    use std::convert::Infallible;

    let address = address.into();

    let service = make_service_fn(move |_| {
        let registry = registry.clone();
        async move { Ok::<_, Infallible>(service_fn(move |_| prometheus_serve(registry.clone()))) }
    });

    let server = hyper::Server::bind(&address).serve(service);
    tokio::spawn(server);
}
