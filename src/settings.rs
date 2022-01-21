// Copyright 2022 joshyrobot, ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling of bot configuration for hosters.

use config::{Config, ConfigError, Environment, File};
use log::LevelFilter;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serenity::model::id::GuildId;
#[cfg(feature = "reporting")]
use url::Url;

use std::{collections::HashMap, env, path::PathBuf};

#[cfg(feature = "bot")]
use std::time::Duration;

#[cfg(feature = "bot")]
mod duration_de {
	use serde::{de, Deserializer};

	use std::{fmt, time::Duration};
	/// Visitor to deserialize a `Duration` from a number of seconds.
	struct DurationVisitor;
	impl<'de> de::Visitor<'de> for DurationVisitor {
		type Value = Duration;
		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			write!(formatter, "a std::time::Duration in seconds")
		}
		fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
		where
			E: de::Error,
		{
			Ok(Duration::from_secs(v))
		}
	}
	pub(super) fn deserialize_duration<'de, D>(
		d: D,
	) -> Result<Duration, D::Error>
	where
		D: Deserializer<'de>,
	{
		d.deserialize_u64(DurationVisitor)
	}
}

#[cfg(feature = "bot")]
use duration_de::deserialize_duration;

#[cfg(feature = "monitoring")]
mod user_address {
	use serde::{de, Deserialize, Deserializer};
	use std::{
		fmt,
		net::{SocketAddr, ToSocketAddrs},
	};
	#[derive(Debug)]
	pub(crate) struct UserAddress {
		pub(crate) socket_addr: SocketAddr,
	}

	impl<'de> Deserialize<'de> for UserAddress {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			deserializer.deserialize_str(UserAddressVisitor)
		}
	}

	/// Visitor to deserialize a `SocketAddr` using ToSocketAddrs.
	struct UserAddressVisitor;
	impl<'de> de::Visitor<'de> for UserAddressVisitor {
		type Value = UserAddress;
		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			write!(formatter, "a socket address in the form `host:port`")
		}

		fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
		where
			E: de::Error,
		{
			let socket_addr = v
				.to_socket_addrs()
				.map_err(E::custom)?
				.into_iter()
				.next()
				.ok_or_else(|| {
					E::custom("provided host did not resolve to an address")
				})?;

			Ok(UserAddress { socket_addr })
		}
	}
}

#[cfg(feature = "monitoring")]
pub(crate) use user_address::UserAddress;

/// Settings for the highlighting behavior of the bot.
#[derive(Debug, Deserialize)]
pub(crate) struct BehaviorSettings {
	/// Maximum number of keywords allowed for one user.
	pub(crate) max_keywords: u32,

	/// Duration to wait for activity before sending a notification.
	#[serde(
		rename = "patience_seconds",
		deserialize_with = "deserialize_duration"
	)]
	#[cfg(feature = "bot")]
	pub(crate) patience: Duration,
}

/// Settings for the account of the bot.
#[cfg(feature = "bot")]
#[derive(Debug, Deserialize)]
pub(crate) struct BotSettings {
	/// Bot token to log into Discord with.
	pub(crate) token: String,
	/// ID of the bot's application.
	pub(crate) application_id: u64,
	/// Whether this bot is private or not.
	///
	/// Controls whether the `about` command outputs an invite link.
	pub(crate) private: bool,
	pub(crate) test_guild: Option<GuildId>,
}

/// Settings for various logging facilities.
#[derive(Debug, Deserialize)]
pub(crate) struct LoggingSettings {
	/// Webhook URL to send error/panic messages to.
	#[cfg(feature = "reporting")]
	pub(crate) webhook: Option<Url>,
	/// Address to host an HTTP server for prometheus to scrape.
	#[cfg(feature = "monitoring")]
	pub(crate) prometheus: Option<UserAddress>,

	/// Global level that log messages should be filtered to.
	pub(crate) level: LevelFilter,
	/// Per-module log level filters.
	pub(crate) filters: HashMap<String, LevelFilter>,
}

/// Settings for the database.
#[derive(Debug, Deserialize)]
pub(crate) struct DatabaseSettings {
	/// Path to the directory that should hold the database.
	pub(crate) path: PathBuf,
	/// Whether or not to run automatic daily backups.
	pub(crate) backup: bool,
}

/// Collection of settings.
#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
	pub(crate) behavior: BehaviorSettings,
	#[cfg(feature = "bot")]
	pub(crate) bot: BotSettings,
	pub(crate) logging: LoggingSettings,
	pub(crate) database: DatabaseSettings,
}

impl Settings {
	/// Builds settings from environment variables and the configuration file.
	pub(crate) fn new() -> Result<Self, ConfigError> {
		let mut s = Config::new();

		s.set_default("behavior.max_keywords", 100)?;
		#[cfg(feature = "bot")]
		s.set_default("behavior.patience_seconds", 60 * 2)?;

		#[cfg(feature = "bot")]
		s.set_default("bot.private", false)?;

		s.set_default("logging.level", "WARN")?;
		s.set_default("logging.filters.highlights", "INFO")?;

		s.set_default("database.path", "./data")?;
		s.set_default("database.backup", true)?;

		let filename = env::var("HIGHLIGHTS_CONFIG")
			.unwrap_or_else(|_| "./config.toml".to_owned());
		s.merge(File::with_name(&filename).required(false)).unwrap();

		s.merge(Environment::with_prefix("HIGHLIGHTS"))?;

		s.try_into()
	}
}

/// Settings configured by the hoster.
static SETTINGS: OnceCell<Settings> = OnceCell::new();

/// Gets the settings configured by the hoster.
pub(crate) fn settings() -> &'static Settings {
	SETTINGS.get().expect("Settings were not initialized")
}

/// Initialize the bot's [`Settings`](Settings).
pub(crate) fn init() {
	match Settings::new() {
		Ok(settings) => {
			let _ = SETTINGS.set(settings);
		}
		Err(e) => {
			panic!("Failed to parse settings: {}", e);
		}
	}
}
