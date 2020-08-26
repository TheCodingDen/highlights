use env_logger::Logger as EnvLogger;
use log::{Level, Log, Metadata, Record};
use once_cell::sync::OnceCell;
use reqwest::{
	blocking::{self, Client as BlockingClient},
	Client as AsyncClient, Url,
};
use serde::Serialize;

use std::{env, panic, time::Duration};

use crate::Error;

static WEBHOOK_URL: OnceCell<Url> = OnceCell::new();

static WEBHOOK_CLIENT: OnceCell<AsyncClient> = OnceCell::new();

#[derive(Serialize)]
struct WebhookMessage {
	content: String,
}

struct Logger {
	inner: EnvLogger,
}

impl Log for Logger {
	fn enabled(&self, meta: &Metadata) -> bool {
		meta.level() == Level::Error || self.inner.enabled(meta)
	}

	fn log(&self, record: &Record) {
		if self.inner.enabled(record.metadata()) {
			self.inner.log(record);
		}

		if record.level() == Level::Error {
			let content = format!("[{}] {}", record.level(), record.args());
			tokio::spawn(async move {
				if let Err(e) = report_error(content).await {
					log::warn!("Failed to report error: {}", e);
				}
			});
		}
	}

	fn flush(&self) {
		self.inner.flush();
	}
}

pub fn init() {
	let mut env_logger_builder = env_logger::from_env(
		env_logger::Env::new()
			.filter_or("HIGHLIGHTS_LOG_FILTER", "highlights=info")
			.write_style("HIGHLIGHTS_LOG_STYLE"),
	);

	match env::var("HIGHLIGHTS_WEBHOOK_URL") {
		Ok(url) => match url.parse() {
			Ok(url) => WEBHOOK_URL.set(url).unwrap(),
			Err(e) => {
				log::error!(
					"HIGHLIGHTS_WEBHOOK_URL is an invalid URL ({}): {}",
					url,
					e
				);
				env_logger_builder.init();
				return;
			}
		},
		Err(env::VarError::NotPresent) => {
			log::warn!(
				"HIGHLIGHTS_WEBHOOK_URL is not present, not reporting errors"
			);
			env_logger_builder.init();
			return;
		}
		Err(env::VarError::NotUnicode(_)) => {
			log::error!("HIGHLIGHTS_WEBHOOK_URL is invalid UTF-8");
			env_logger_builder.init();
			return;
		}
	}

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

	let env_logger = env_logger_builder.build();

	let max_level = env_logger.filter();

	let logger = Logger { inner: env_logger };

	log::set_boxed_logger(Box::new(logger)).expect("Failed to set logger");

	log::set_max_level(max_level);
}

async fn report_error(content: String) -> Result<reqwest::Response, Error> {
	let url = WEBHOOK_URL.get().ok_or("Webhook URL not set")?.to_owned();
	let client = WEBHOOK_CLIENT.get().ok_or("Webhook client not set")?;

	let message = WebhookMessage { content };

	Ok(client
		.post(url)
		.json(&message)
		.timeout(Duration::from_secs(5))
		.send()
		.await?)
}

fn report_panic(info: &panic::PanicInfo) -> Result<blocking::Response, Error> {
	let url = WEBHOOK_URL.get().ok_or("Webhook URL not set")?.to_owned();
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
