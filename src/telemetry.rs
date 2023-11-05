use std::fmt::Display;

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
