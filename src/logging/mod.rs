// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Logging setup using [`tracing`].

use anyhow::Result;
use tracing::{warn, Metadata};
use tracing_subscriber::{
	filter::FilterFn,
	layer::{Layer, SubscriberExt},
	util::SubscriberInitExt,
};

use crate::settings::{settings, Settings};

#[cfg(feature = "monitoring")]
mod monitoring;

#[cfg(feature = "reporting")]
mod reporting;

/// Applies configured filters to the given tracing metadata.
///
/// Returns true if the metadata passed configured filters and should be logged,
/// and false if it should be filtered out.
///
/// Uses [`LoggingSettings::level`](crate::settings::LoggingSettings::level) and
/// [`LoggingSettings::filters`](crate::settings::LoggingSettings::filters).
fn use_filters(settings: &Settings, metadata: &Metadata) -> bool {
	std::iter::successors(metadata.module_path(), |path| {
		path.rsplit_once("::").map(|(prefix, _)| prefix)
	})
	.filter_map(|path| {
		settings
			.logging
			.filters
			.get(path)
			.map(|filter| filter >= metadata.level())
	})
	.chain(Some(&settings.logging.level >= metadata.level()))
	.next()
	.unwrap_or(true)
}

/// Initializes logging via [`tracing`].
///
/// This initializes [`reporting`] and [`monitoring`], if
/// enabled, as well as basic stdout logging.
pub(crate) fn init() -> Result<()> {
	let subscriber = tracing_subscriber::registry().with(
		tracing_subscriber::fmt::layer()
			.with_ansi(settings().logging.color)
			.with_filter({
				let settings = settings();
				FilterFn::new(|metadata| use_filters(settings, metadata))
			}),
	);

	#[cfg(feature = "monitoring")]
	let (is_monitoring, subscriber) = {
		let layer = monitoring::init()?;
		(layer.is_some(), subscriber.with(layer))
	};

	#[cfg(feature = "reporting")]
	let (is_reporting, subscriber) = {
		let layer = reporting::init();
		(layer.is_some(), subscriber.with(layer))
	};

	subscriber.try_init()?;

	#[cfg(feature = "monitoring")]
	if !is_monitoring {
		warn!("Jaeger agent address not provided; not reporting traces");
	}

	#[cfg(feature = "reporting")]
	if !is_reporting {
		warn!("Webhook URL is not present, not reporting panics");
	}

	Ok(())
}
