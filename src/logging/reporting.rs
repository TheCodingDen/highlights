// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Error and panic reporting to a Discord webhook.

use anyhow::Result;
use reqwest::{
	blocking::{self, Client as BlockingClient},
	Client, Url,
};
use serde::Serialize;
use tracing::{
	metadata::LevelFilter,
	span::{Attributes, Record},
	Event, Id, Subscriber,
};
use tracing_subscriber::{
	fmt::{format::DefaultFields, FormatFields, FormattedFields},
	layer::{Context, Layer},
	registry::LookupSpan,
};

use crate::settings::settings;

use std::{fmt::Write, panic, time::Duration};

/// Message that can be serialized to be sent to a webhook.
#[derive(Serialize)]
struct WebhookMessage {
	content: String,
}

pub(crate) struct WebhookLayer {
	url: Url,
	client: Client,
}

impl WebhookLayer {
	pub(super) fn new(url: Url) -> Self {
		WebhookLayer {
			url,
			client: Client::new(),
		}
	}
}

fn format_event<S>(event: &Event<'_>, ctx: Context<'_, S>) -> String
where
	S: Subscriber + for<'s> LookupSpan<'s>,
{
	let metadata = event.metadata();
	let mut contents = "**[ERROR]** ".to_owned();

	if let Some(scope) = ctx.event_scope(event) {
		for span in scope.from_root() {
			if let Some(fields) =
				span.extensions().get::<FormattedFields<DefaultFields>>()
			{
				let _ = write!(contents, "__{}__", span.name());
				if !fields.is_empty() {
					let _ = write!(contents, "{{*{}*}}", fields);
				}

				contents.push_str(": ");
			}
		}
	}

	if let Some(file) = metadata.file() {
		let _ = write!(contents, "*{}:", file);

		if let Some(line) = metadata.line() {
			let _ = write!(contents, "{}:* ", line);
		} else {
			contents.push_str("* ");
		}
	}

	let _ = write!(contents, "__{}__: ", metadata.target());

	let mut formatter = FormattedFields::<DefaultFields>::new(contents);

	let writer = formatter.as_writer();

	let _ = DefaultFields::new().format_fields(writer, event);

	formatter.fields
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for WebhookLayer {
	fn on_new_span(
		&self,
		attrs: &Attributes<'_>,
		id: &Id,
		ctx: Context<'_, S>,
	) {
		let span = ctx.span(id).expect("Couldn't get span for attributes");
		let mut extensions = span.extensions_mut();

		if extensions
			.get_mut::<FormattedFields<DefaultFields>>()
			.is_none()
		{
			let mut fields =
				FormattedFields::<DefaultFields>::new(String::new());
			if DefaultFields::new()
				.format_fields(fields.as_writer(), attrs)
				.is_ok()
			{
				extensions.insert(fields);
			}
		}
	}

	fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
		let span = ctx.span(id).expect("Couldn't get span for record");
		let mut extensions = span.extensions_mut();

		if let Some(fields) =
			extensions.get_mut::<FormattedFields<DefaultFields>>()
		{
			let _ = DefaultFields::new().add_fields(fields, values);
			return;
		}

		let mut fields = FormattedFields::<DefaultFields>::new(String::new());
		if DefaultFields::new()
			.format_fields(fields.as_writer(), values)
			.is_ok()
		{
			extensions.insert(fields);
		}
	}

	fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
		if &LevelFilter::ERROR < event.metadata().level() {
			return;
		}

		let message = WebhookMessage {
			content: format_event(event, ctx),
		};

		let client = self.client.clone();
		let url = self.url.clone();

		tokio::spawn(async move {
			if let Err(e) = client
				.post(url)
				.json(&message)
				.timeout(Duration::from_secs(5))
				.send()
				.await
			{
				tracing::warn!("Error reporting error: {}", e)
			}
		});
	}
}

/// Reports a panic to the configured webhook URL.
pub(crate) fn report_panic(
	info: &panic::PanicInfo,
	url: Url,
) -> Result<blocking::Response> {
	let client = BlockingClient::builder().build()?;

	let message = WebhookMessage {
		content: format!("**[PANIC]** {}", info),
	};

	Ok(client
		.post(url)
		.json(&message)
		.timeout(Duration::from_secs(5))
		.send()?)
}

pub(crate) fn init() -> Option<WebhookLayer> {
	if let Some(url) = settings().logging.webhook.clone() {
		let default_panic_hook = panic::take_hook();

		let reporting_panic_hook: Box<
			dyn Fn(&panic::PanicInfo<'_>) + Send + Sync + 'static,
		> = {
			let url = url.clone();
			Box::new(move |info| {
				if let Err(e) = report_panic(info, url.clone()) {
					tracing::error!("Error reporting panic: {}", e);
				}

				default_panic_hook(info);
			})
		};

		panic::set_hook(reporting_panic_hook);

		Some(WebhookLayer::new(url))
	} else {
		None
	}
}
