// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Error and panic reporting to a Discord webhook.

use log::LevelFilter;
use simplelog::{
	ColorChoice, CombinedLogger, ConfigBuilder, SharedLogger, TermLogger,
	TerminalMode,
};

use crate::settings::settings;

#[cfg(feature = "reporting")]
mod webhook {
	use std::{panic, time::Duration};

	use anyhow::{Context as _, Result};
	use log::{Level, LevelFilter, Log, Metadata, Record};
	use once_cell::sync::OnceCell;
	use reqwest::{
		blocking::{self, Client as BlockingClient},
		Client as AsyncClient,
	};
	use serde::Serialize;
	use simplelog::{Config, SharedLogger};

	use crate::settings::settings;

	/// Global client to use when sending webhook messages.
	pub static WEBHOOK_CLIENT: OnceCell<AsyncClient> = OnceCell::new();

	/// Message that can be serialized to be sent to a webhook.
	#[derive(Serialize)]
	struct WebhookMessage {
		content: String,
	}

	pub struct WebhookLogger;
	impl Log for WebhookLogger {
		fn enabled(&self, meta: &Metadata) -> bool {
			meta.level() == Level::Error
		}

		fn log(&self, record: &Record) {
			if self.enabled(record.metadata()) {
				let content = format!("[{}] {}", record.level(), record.args());
				tokio::spawn(async move {
					if let Err(e) = report_error(content).await {
						log::warn!("Failed to report error: {}", e);
					}
				});
			}
		}

		fn flush(&self) {}
	}
	impl SharedLogger for WebhookLogger {
		fn level(&self) -> LevelFilter {
			LevelFilter::Error
		}

		fn config(&self) -> Option<&Config> {
			None
		}

		fn as_log(self: Box<Self>) -> Box<dyn Log> {
			Box::new(*self)
		}
	}

	/// Reports a logged error to the configured webhook URL.
	async fn report_error(content: String) -> Result<reqwest::Response> {
		let url = settings()
			.logging
			.webhook
			.as_ref()
			.context("Webhook URL not set")?
			.to_owned();
		let client = WEBHOOK_CLIENT.get().context("Webhook client not set")?;

		let message = WebhookMessage { content };

		Ok(client
			.post(url)
			.json(&message)
			.timeout(Duration::from_secs(5))
			.send()
			.await?)
	}

	/// Reports a panic to the configured webhook URL.
	pub fn report_panic(info: &panic::PanicInfo) -> Result<blocking::Response> {
		let url = settings()
			.logging
			.webhook
			.as_ref()
			.context("Webhook URL not set")?
			.to_owned();
		let client = BlockingClient::builder().build()?;

		let message = WebhookMessage {
			content: format!("[PANIC] {}", info),
		};

		Ok(client
			.post(url)
			.json(&message)
			.timeout(Duration::from_secs(5))
			.send()?)
	}
}

#[cfg(feature = "reporting")]
pub fn init() {
	use std::panic;

	use reqwest::Client as AsyncClient;

	use webhook::*;

	let mut loggers = term_logger();

	if settings().logging.webhook.is_some() {
		WEBHOOK_CLIENT
			.set(
				AsyncClient::builder()
					.build()
					.expect("Failed to build webhook client"),
			)
			.unwrap();

		let default_panic_hook = panic::take_hook();

		let reporting_panic_hook: Box<
			dyn Fn(&panic::PanicInfo<'_>) + Send + Sync + 'static,
		> = Box::new(move |info| {
			if let Err(e) = report_panic(info) {
				log::error!("Error reporting panic: {}", e);
			}

			default_panic_hook(info);
		});

		panic::set_hook(reporting_panic_hook);

		loggers.push(Box::new(WebhookLogger));

		CombinedLogger::init(loggers).expect("Failed to set logger");
	} else {
		CombinedLogger::init(loggers).expect("Failed to set logger");

		log::warn!("Webhook URL is not present, not reporting errors");
	}
}

#[cfg(not(feature = "reporting"))]
pub fn init() {
	CombinedLogger::init(term_logger()).expect("Failed to set logger");
}

fn term_logger() -> Vec<Box<dyn SharedLogger>> {
	let mut loggers: Vec<Box<dyn SharedLogger>> = vec![];

	let mut config_builder = ConfigBuilder::new();

	config_builder.set_target_level(LevelFilter::Error);

	for (path, level) in &settings().logging.filters {
		config_builder.add_filter_ignore(path.to_string());

		let mut config = ConfigBuilder::new();
		config.set_target_level(LevelFilter::Error);
		config.add_filter_allow(path.to_string());
		loggers.push(TermLogger::new(
			*level,
			config.build(),
			TerminalMode::Mixed,
			ColorChoice::Auto,
		));
	}

	loggers
}
