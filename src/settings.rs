// Copyright 2022 joshyrobot, ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling of bot configuration for hosters.

#[cfg(feature = "sqlite")]
use std::path::PathBuf;
#[cfg(feature = "bot")]
use std::time::Duration;
use std::{
	collections::HashMap,
	env::{self, VarError},
	fs::read_to_string,
	io::ErrorKind,
};

use config::{
	builder::DefaultState, ConfigBuilder, ConfigError, Environment, File,
	FileFormat,
};
use once_cell::sync::OnceCell;
use serde::Deserialize;
#[cfg(feature = "bot")]
use serenity::model::id::GuildId;
use tracing::metadata::LevelFilter;
use url::Url;

#[cfg(feature = "bot")]
mod duration_de {
	use std::{fmt, time::Duration};

	use serde::{de, Deserializer};

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
	use std::{
		fmt,
		net::{SocketAddr, ToSocketAddrs},
	};

	use serde::{de, Deserialize, Deserializer};

	#[derive(Debug, Clone, Copy)]
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

mod level {
	use std::{collections::HashMap, fmt};

	use serde::{de, Deserialize, Deserializer};
	use tracing::metadata::LevelFilter;

	struct LevelFilterWrapper(LevelFilter);

	/// Visitor to deserialize a `LevelFilter` from a string.
	struct LevelFilterVisitor;
	impl<'de> de::Visitor<'de> for LevelFilterVisitor {
		type Value = LevelFilterWrapper;
		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			write!(
				formatter,
				"a logging level (trace, debug, info, warn, error, off)"
			)
		}

		fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
		where
			E: de::Error,
		{
			match v {
				"off" | "OFF" => Ok(LevelFilter::OFF),
				"trace" | "TRACE" => Ok(LevelFilter::TRACE),
				"debug" | "DEBUG" => Ok(LevelFilter::DEBUG),
				"info" | "INFO" => Ok(LevelFilter::INFO),
				"warn" | "WARN" => Ok(LevelFilter::WARN),
				"error" | "ERROR" => Ok(LevelFilter::ERROR),
				_ => Err(E::invalid_value(de::Unexpected::Str(v), &self)),
			}
			.map(LevelFilterWrapper)
		}
	}

	impl<'de> Deserialize<'de> for LevelFilterWrapper {
		fn deserialize<D>(d: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			d.deserialize_str(LevelFilterVisitor)
		}
	}

	pub(super) fn deserialize_level_filter<'de, D>(
		d: D,
	) -> Result<LevelFilter, D::Error>
	where
		D: Deserializer<'de>,
	{
		LevelFilterWrapper::deserialize(d).map(|LevelFilterWrapper(f)| f)
	}

	/// Visitor to deserialize a `LevelFilter` from a string.
	struct LevelFiltersVisitor;
	impl<'de> de::Visitor<'de> for LevelFiltersVisitor {
		type Value = HashMap<String, LevelFilter>;
		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			write!(formatter, "a table of modules to logging levels")
		}

		fn visit_map<A>(self, mut filters: A) -> Result<Self::Value, A::Error>
		where
			A: de::MapAccess<'de>,
		{
			let mut map = HashMap::new();

			while let Some((module, LevelFilterWrapper(filter))) =
				filters.next_entry::<String, LevelFilterWrapper>()?
			{
				map.insert(module, filter);
			}

			Ok(map)
		}
	}

	pub(super) fn deserialize_level_filters<'de, D>(
		d: D,
	) -> Result<HashMap<String, LevelFilter>, D::Error>
	where
		D: Deserializer<'de>,
	{
		d.deserialize_map(LevelFiltersVisitor)
	}
}

use level::{deserialize_level_filter, deserialize_level_filters};
#[cfg(feature = "monitoring")]
pub(crate) use user_address::UserAddress;

/// Settings for the highlighting behavior of the bot.
#[cfg(feature = "bot")]
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
	/// Address to find Jaeger agent to send traces to.
	#[cfg(feature = "monitoring")]
	pub(crate) jaeger: Option<UserAddress>,

	/// Percentage of traces to sample.
	///
	/// See [`TraceIdRatioBased`](opentelemetry::sdk::trace::Sampler::TraceIdRatioBased).
	#[cfg(feature = "monitoring")]
	pub(crate) sample_ratio: f64,

	/// Global level that log messages should be filtered to.
	#[serde(deserialize_with = "deserialize_level_filter")]
	pub(crate) level: LevelFilter,
	/// Per-module log level filters.
	#[serde(deserialize_with = "deserialize_level_filters")]
	pub(crate) filters: HashMap<String, LevelFilter>,

	/// Whether or not to use ANSI color codes.
	pub(crate) color: bool,
}

/// Settings for the database.
#[derive(Debug, Deserialize)]
pub(crate) struct DatabaseSettings {
	/// Path to the directory that should hold the SQLite database.
	#[cfg(feature = "sqlite")]
	pub(crate) path: Option<PathBuf>,
	/// Database connection URL.
	#[cfg(feature = "sqlite")]
	pub(crate) url: Option<Url>,
	/// Database connection URL.
	#[cfg(not(feature = "sqlite"))]
	pub(crate) url: Url,
	/// Whether or not to run automatic daily backups.
	#[cfg(feature = "backup")]
	pub(crate) backup: Option<bool>,
}

/// Collection of settings.
#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
	#[cfg(feature = "bot")]
	pub(crate) behavior: BehaviorSettings,
	#[cfg(feature = "bot")]
	pub(crate) bot: BotSettings,
	pub(crate) logging: LoggingSettings,
	pub(crate) database: DatabaseSettings,
}

impl Settings {
	/// Builds settings from environment variables and the configuration file.
	pub(crate) fn new() -> Result<Self, ConfigError> {
		let b = ConfigBuilder::<DefaultState>::default();

		#[cfg(feature = "bot")]
		let b = b.set_default("behavior.max_keywords", 100i64)?
			.set_default("behavior.patience_seconds", 60i64 * 2)?
			.set_default("bot.private", false)?;

		#[cfg(feature = "monitoring")]
		let b = b.set_default("logging.sample_ratio", 1.0f64)?;

		let mut b = b
			.set_default("logging.level", "WARN")?
			.set_default("logging.filters.highlights", "INFO")?
			.set_default("logging.color", "true")?;

		let filename = env::var("HIGHLIGHTS_CONFIG").or_else(|e| match e {
			VarError::NotPresent => Ok("./config.toml".to_owned()),
			e => Err(ConfigError::Foreign(Box::new(e))),
		})?;
		match read_to_string(&filename) {
			Ok(conf) => {
				b = b.add_source(File::from_str(&conf, FileFormat::Toml));
			}
			Err(e) if e.kind() == ErrorKind::NotFound => (),
			Err(e) => return Err(ConfigError::Foreign(Box::new(e))),
		}

		b.add_source(Environment::with_prefix("HIGHLIGHTS"))
			.build()?
			.try_deserialize()
	}
}

/// Settings configured by the hoster.
static SETTINGS: OnceCell<Settings> = OnceCell::new();

/// Gets the settings configured by the hoster.
pub(crate) fn settings() -> &'static Settings {
	SETTINGS.get().expect("Settings were not initialized")
}

/// Initialize the bot's [`Settings`].
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
