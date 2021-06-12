// Copyright 2021 joshyrobot, ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling of bot configuration for hosters.

use config::{Config, ConfigError, Environment, File};
use log::LevelFilter;
use once_cell::sync::OnceCell;
use serde::{de, Deserialize, Deserializer};
use url::Url;

use std::{
	collections::HashMap, env, fmt, net::SocketAddr, path::PathBuf,
	time::Duration,
};

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
fn deserialize_duration<'de, D>(d: D) -> Result<Duration, D::Error>
where
	D: Deserializer<'de>,
{
	d.deserialize_u64(DurationVisitor)
}

/// Settings for the highlighting behavior of the bot.
#[derive(Debug, Deserialize)]
pub struct BehaviorSettings {
	/// Maximum number of keywords allowed for one user.
	pub max_keywords: u32,

	#[serde(
		rename = "patience_seconds",
		deserialize_with = "deserialize_duration"
	)]
	/// Duration to wait for activity before sending a notification.
	pub patience: Duration,
}

/// Settings for the account of the bot.
#[derive(Debug, Deserialize)]
pub struct BotSettings {
	/// Bot token to log into Discord with.
	pub token: String,
	/// Whether this bot is private or not.
	///
	/// Controls whether the `about` command outputs an invite link.
	pub private: bool,
}

/// Settings for various logging facilities.
#[derive(Debug, Deserialize)]
pub struct LoggingSettings {
	/// Webhook URL to send error/panic messages to.
	pub webhook: Option<Url>,
	/// Address to host an HTTP server for prometheus to scrape.
	pub prometheus: Option<SocketAddr>,

	/// Global level that log messages should be filtered to.
	pub level: LevelFilter,
	/// Per-module log level filters.
	pub filters: HashMap<String, LevelFilter>,
}

/// Settings for the database.
#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
	/// Path to the directory that should hold the database.
	pub path: PathBuf,
	/// Whether or not to run automatic daily backups.
	pub backup: bool,
}

/// Collection of settings.
#[derive(Debug, Deserialize)]
pub struct Settings {
	pub behavior: BehaviorSettings,
	pub bot: BotSettings,
	pub logging: LoggingSettings,
	pub database: DatabaseSettings,
}

impl Settings {
	/// Builds settings from environment variables and the configuration file.
	pub fn new() -> Result<Self, ConfigError> {
		let mut s = Config::new();

		s.set_default("behavior.max_keywords", 100)?;
		s.set_default("behavior.patience_seconds", 60 * 2)?;

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
pub fn settings() -> &'static Settings {
	SETTINGS.get().expect("Settings were not initialized")
}

/// Initialize the bot's [`Settings`](Settings).
pub fn init() {
	match Settings::new() {
		Ok(settings) => {
			let _ = SETTINGS.set(settings);
		}
		Err(e) => {
			panic!("Failed to parse settings: {}", e);
		}
	}
}
