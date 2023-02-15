// Copyright 2023 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Highlights is a simple but flexible keyword highlighting bot for Discord.
//!
//! The code for highlights is organized into mostly independent modules. This
//! module handles creating the client and registering event listeners.

#![allow(clippy::tabs_in_doc_comments)]

use anyhow::Result;
use tracing::warn;

use crate::settings::settings;

pub(crate) mod db;

pub(crate) mod settings;

#[cfg(feature = "bot")]
pub(crate) mod global;

pub(crate) mod logging;

#[cfg(feature = "bot")]
mod bot;

/// Entrypoint function to initialize other modules.
#[tokio::main]
async fn main() -> Result<()> {
	settings::init()?;

	logging::init()?;

	if settings().behavior.patience_seconds.is_some() {
		warn!(
			"Your configuration includes behavior.patience_seconds. \
			This setting is deprecated; please use behavior.patience instead. \
			For example, patience = \"2m\"."
		);
	}

	db::init().await?;

	#[cfg(feature = "bot")]
	bot::init().await?;

	#[cfg(not(feature = "bot"))]
	futures_util::future::pending::<()>().await;

	Ok(())
}
