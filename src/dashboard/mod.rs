// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

use std::time::Duration;

use dashmap::DashMap;
use http::{
	header::{LOCATION, SET_COOKIE},
	Response, StatusCode,
};
use once_cell::sync::{Lazy, OnceCell};
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;
use warp::{Filter, Rejection, Reply};

use crate::settings::settings;

mod assets;

mod session;

use session::{Session, UuidExt as _};

static REDIRECT_URL: OnceCell<String> = OnceCell::new();

static HTTP_CLIENT: OnceCell<Client> = OnceCell::new();

static CLIENT_AUTHORIZATION: OnceCell<String> = OnceCell::new();

static USES_TLS: OnceCell<bool> = OnceCell::new();

fn redirect_url() -> &'static str {
	REDIRECT_URL.get().expect("REDIRECT_URL not set").as_str()
}

fn client() -> &'static Client {
	HTTP_CLIENT.get().expect("HTTP_CLIENT not set")
}

fn client_authorization() -> &'static str {
	CLIENT_AUTHORIZATION
		.get()
		.expect("CLIENT_AUTHORIZATION not set")
		.as_str()
}

fn uses_tls() -> bool {
	*USES_TLS.get().expect("USES_TLS not set")
}

fn endpoint(endpoint: &str) -> &'static str {
	const DISCORD_API: &str = "https://discordapp.com/api/v8/";

	#[allow(clippy::type_complexity)]
	static ENDPOINTS: Lazy<
		DashMap<&str, &str>,
		fn() -> DashMap<&'static str, &'static str>,
	> = Lazy::new(DashMap::new);

	if let Some(url) = ENDPOINTS.get(endpoint) {
		*url
	} else {
		let url: &'static str = &*Box::leak(
			format!("{}{}", DISCORD_API, endpoint).into_boxed_str(),
		);

		let endpoint: &'static str =
			&*Box::leak(endpoint.to_owned().into_boxed_str());

		*ENDPOINTS.entry(endpoint).or_insert(url)
	}
}

pub fn init() {
	let settings = match &settings().dashboard {
		Some(s) => s,
		None => {
			log::warn!(
				"Dashboard settings not provided; \
				not starting dashboard web server"
			);
			return;
		}
	};

	USES_TLS
		.set(
			settings.tls.is_some()
				|| settings
					.base_url
					.as_ref()
					.map_or(false, |url| url.scheme() == "https"),
		)
		.expect("USES_TLS already set");

	REDIRECT_URL
		.set(
			settings
				.base_url
				.as_ref()
				.map_or_else(
					|| {
						format!(
							"{}://{}/callback",
							if settings.tls.is_some() {
								"https"
							} else {
								"http"
							},
							settings.address.raw
						)
						.parse()
						.expect("Failed to parse default URL")
					},
					|base| {
						base.join("callback")
							.expect("Failed to join to base URL")
					},
				)
				.to_string(),
		)
		.expect("REDIRECT_URL already set");

	HTTP_CLIENT
		.set(
			Client::builder()
				.timeout(Duration::from_secs(10))
				.build()
				.expect("Failed to build HTTP client"),
		)
		.expect("HTTP_CLIENT already set");

	CLIENT_AUTHORIZATION
		.set(format!(
			"Basic {}",
			base64::encode(format!(
				"{}:{}",
				settings.client_id, settings.client_secret
			))
		))
		.expect("CLIENT_AUTHORIZATION already set");

	if let Some(tls_settings) = settings.tls.as_ref() {
		tokio::spawn(
			warp::serve(routes())
				.tls()
				.key_path(&tls_settings.key)
				.cert_path(&tls_settings.cert)
				.run(settings.address.socket_addr),
		);
	} else {
		tokio::spawn(warp::serve(routes()).run(settings.address.socket_addr));
	}
}

#[derive(Debug, Deserialize)]
struct OAuthCallback {
	code: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

fn routes() -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
	#[derive(Debug, Deserialize)]
	struct SessionId {
		s: String,
	}
	warp::get()
		.and(
			warp::path("callback")
				.and(warp::path::end())
				.and(warp::query())
				.and_then(|callback: OAuthCallback| async move {
					log::debug!("Endpoint called: /callback");

					let cookie = Session::create(callback)
						.await
						.map_err(AnyhowRejection)?;

					Ok::<_, Rejection>(
						Response::builder()
							.status(StatusCode::SEE_OTHER)
							.header(LOCATION, "/dashboard")
							.header(SET_COOKIE, cookie.to_string())
							.body("")
							.expect("Failed to create redirection response"),
					)
				}),
		)
		.or(warp::path("dashboard")
			.and(warp::path::end())
			.and(warp::cookie::optional("s"))
			.and_then(|session_id: Option<String>| async move {
				log::debug!("Endpoint called: /dashboard");

				let response = if let Some(id) = session_id {
					let uuid = Uuid::decode_base64(id.as_bytes())?;
					format!("Session ID: {}", uuid)
				} else {
					"No session ID".to_owned()
				};

				Ok::<_, Rejection>(response)
			}))
}

#[derive(Debug)]
struct AnyhowRejection(anyhow::Error);

impl warp::reject::Reject for AnyhowRejection {}
