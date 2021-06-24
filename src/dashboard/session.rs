// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

use anyhow::{Context as _, Result};
use chrono::{DateTime, Duration, Utc};
use cookie::Cookie;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use time::Duration as NewDuration;
use uuid::Uuid;

use std::convert::TryFrom;

use crate::dashboard::{
	client_authorization, endpoint, redirect_url, uses_tls, OAuthCallback,
};

#[derive(Debug)]
pub struct Session {
	pub id: Uuid,
	pub access_token: String,
	expires_at: DateTime<Utc>,
}

impl Session {
	pub(super) async fn create(
		callback: OAuthCallback,
	) -> Result<Cookie<'static>> {
		#[derive(Debug, Deserialize)]
		struct AccessTokenResponse {
			access_token: String,
			expires_in: u32,
		}

		#[derive(Debug, Serialize)]
		struct OAuthGrantArgs {
			grant_type: &'static str,
			code: String,
			redirect_uri: &'static str,
		}

		let res = super::client()
			.post(endpoint("oauth2/token"))
			.form(&OAuthGrantArgs {
				grant_type: "authorization_code",
				code: callback.code,
				redirect_uri: redirect_url(),
			})
			.header("Authorization", client_authorization())
			.send()
			.await?
			.text()
			.await?;

		let AccessTokenResponse {
			access_token,
			expires_in,
		} = serde_json::from_str(&res)
			.with_context(|| format!("Unexpected response: {:?}", res))?;

		let id = Uuid::new_v4();

		let this = Self {
			id,
			access_token,
			expires_at: Utc::now() + Duration::seconds(expires_in.into()),
		};

		SESSIONS.insert(id, this);

		let cookie = Cookie::build("s", id.encode_base64())
			.max_age(NewDuration::seconds(expires_in.into()))
			.secure(uses_tls())
			.same_site(cookie::SameSite::Lax)
			.http_only(true)
			.finish();

		Ok(cookie)
	}
}

type SessionMap = DashMap<Uuid, Session>;

static SESSIONS: Lazy<SessionMap, fn() -> SessionMap> = Lazy::new(DashMap::new);

#[derive(Debug)]
pub(super) enum InvalidSessionId {
	Base64(base64::DecodeError),
	Length(usize),
}

impl warp::reject::Reject for InvalidSessionId {}

pub(super) trait UuidExt: Sized {
	fn encode_base64(&self) -> String;
	fn decode_base64(bytes: &[u8]) -> Result<Self, InvalidSessionId>;
}

impl UuidExt for Uuid {
	fn encode_base64(&self) -> String {
		base64::encode_config(self.as_bytes(), base64::URL_SAFE_NO_PAD)
	}

	fn decode_base64(bytes: &[u8]) -> Result<Self, InvalidSessionId> {
		let session_id = base64::decode_config(bytes, base64::URL_SAFE_NO_PAD)
			.map_err(InvalidSessionId::Base64)?;

		let bytes = <[u8; 16]>::try_from(session_id.as_slice())
			.map_err(|_| InvalidSessionId::Length(session_id.len()))?;

		Ok(Uuid::from_bytes(bytes))
	}
}
