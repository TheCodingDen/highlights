// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::OnceCell;
use serenity::model::id::UserId;

use std::{env, time::Duration};

pub const MAX_KEYWORDS: u32 = 100;

pub const NOTIFICATION_RETRIES: u8 = 5;

pub const EMBED_COLOR: u32 = 0xefff47;

static BOT_MENTION: OnceCell<String> = OnceCell::new();
static BOT_NICK_MENTION: OnceCell<String> = OnceCell::new();

pub fn bot_mention() -> &'static str {
	BOT_MENTION
		.get()
		.expect("Bot mention was not initialized")
		.as_str()
}

pub fn bot_nick_mention() -> &'static str {
	BOT_NICK_MENTION
		.get()
		.expect("Bot nick mention was not initialized")
		.as_str()
}

pub fn init_mentions(bot_id: UserId) {
	let _ = BOT_MENTION.set(format!("<@{}>", bot_id));
	let _ = BOT_NICK_MENTION.set(format!("<@!{}>", bot_id));
}

static PRIVATE_MODE: OnceCell<bool> = OnceCell::new();

const DEFAULT_PATIENCE_DURATION: Duration = Duration::from_secs(60 * 2);
static PATIENCE_DURATION: OnceCell<Duration> = OnceCell::new();

pub fn private_mode() -> bool {
	*PRIVATE_MODE
		.get()
		.expect("Private mode env was not initialized")
}

pub fn patience_duration() -> Duration {
	*PATIENCE_DURATION
		.get()
		.expect("Patience duration env was not initialized")
}

pub fn init_env() {
	let _ = PRIVATE_MODE.set(env::var_os("HIGHLIGHTS_PRIVATE").is_some());

	let patience_duration = match env::var("HIGHLIGHTS_PATIENCE_SECONDS") {
		Ok(seconds) => match seconds.parse() {
			Ok(seconds) => Some(seconds),
			Err(e) => {
				log::error!(
					"HIGHLIGHTS_PATIENCE_SECONDS is an invalid number ({}): {}",
					seconds,
					e
				);
				None
			}
		},
		Err(env::VarError::NotUnicode(_)) => {
			log::error!("HIGHLIGHTS_PATIENCE_SECONDS is invalid UTF-8");
			None
		}
		Err(env::VarError::NotPresent) => None,
	}
	.map_or(DEFAULT_PATIENCE_DURATION, Duration::from_secs);
	let _ = PATIENCE_DURATION.set(patience_duration);
}
