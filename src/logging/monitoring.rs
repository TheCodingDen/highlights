// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Monitoring with OpenTelemetry and Jaeger

use anyhow::Result;
use opentelemetry::sdk::trace::{self, Sampler, Tracer};
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
	filter::{FilterFn, Filtered},
	layer::Layer as _,
	registry::LookupSpan,
};

use crate::settings::settings;

pub(crate) type Layer<S> = Filtered<OpenTelemetryLayer<S, Tracer>, FilterFn, S>;

pub(crate) fn init<S: Subscriber + for<'span> LookupSpan<'span>>(
) -> Result<Option<Layer<S>>> {
	if let Some(address) = settings().logging.jaeger {
		let tracer = opentelemetry_jaeger::new_pipeline()
			.with_agent_endpoint(address.socket_addr)
			.with_service_name(env!("CARGO_PKG_NAME"))
			.with_trace_config(trace::config().with_sampler(Sampler::AlwaysOn))
			.with_auto_split_batch(true)
			.install_batch(opentelemetry::runtime::Tokio)?;
		let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);

		Ok(Some(opentelemetry.with_filter(FilterFn::new(|metadata| {
			metadata.is_event()
				|| metadata
					.module_path()
					.map_or(true, |path| !path.starts_with("h2::"))
		}))))
	} else {
		Ok(None)
	}
}
