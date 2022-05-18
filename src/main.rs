// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Highlights is a simple but flexible keyword highlighting bot for Discord.
//!
//! The code for highlights is organized into mostly independent modules. This
//! module handles creating the client and registering event listeners.

#![allow(clippy::tabs_in_doc_comments)]

use anyhow::Result;

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
	settings::init();

	logging::init()?;

	db::init();

	#[cfg(feature = "bot")]
	bot::init().await?;

	#[cfg(not(feature = "bot"))]
	futures_util::future::pending::<()>().await;

	Ok(())
}
