// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Highlights is a simple but flexible keyword highlighting bot for Discord.
//!
//! The code for highlights is organized into mostly independent modules. This module handles
//! creating the client and registering event listeners.

#![allow(clippy::tabs_in_doc_comments)]

pub mod db;

pub mod settings;

pub mod global;

pub mod monitoring;

pub mod reporting;

#[cfg(feature = "bot")]
mod bot;

/// Entrypoint function to initialize other modules and start the Discord client.
#[tokio::main]
async fn main() {
	settings::init();

	reporting::init();

	db::init();

	#[cfg(feature = "monitoring")]
	monitoring::init();

	#[cfg(feature = "bot")]
	bot::init().await;

	#[cfg(not(feature = "bot"))]
	futures_util::future::pending::<()>().await;
}
