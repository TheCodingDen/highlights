use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

use log::LevelFilter;
use url::Url;

use std::{
	collections::HashMap, env, net::SocketAddr, path::PathBuf, time::Duration,
};

#[derive(Debug, Deserialize)]
pub struct BehaviorSettings {
	pub max_keywords: u32,
	patience_seconds: u64,
}
impl BehaviorSettings {
	pub fn patience(&self) -> Duration {
		Duration::from_secs(self.patience_seconds)
	}
}

#[derive(Debug, Deserialize)]
pub struct BotSettings {
	pub token: String,
	pub private: bool,
}

#[derive(Debug, Deserialize)]
pub struct LoggingSettings {
	pub webhook: Option<Url>,
	pub prometheus: Option<SocketAddr>,

	pub level: LevelFilter,
	pub filters: HashMap<String, LevelFilter>,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
	pub path: PathBuf,
	pub backup: bool,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
	pub behavior: BehaviorSettings,
	pub bot: BotSettings,
	pub logging: LoggingSettings,
	pub database: DatabaseSettings,
}

impl Settings {
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
			.unwrap_or("./config.toml".to_string());
		s.merge(File::with_name(&filename).required(false)).unwrap();

		s.merge(Environment::with_prefix("HIGHLIGHTS"))?;

		s.try_into()
	}
}
