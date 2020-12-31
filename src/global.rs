// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Global constants, both compile-time and set at runtime.

use once_cell::sync::OnceCell;
use serenity::model::id::UserId;

use crate::settings::Settings;

/// How many times to retry notifications after internal server errors from Discord.
pub const NOTIFICATION_RETRIES: u8 = 5;

/// Color of normal embeds (from help command and notifications).
pub const EMBED_COLOR: u32 = 0xefff47;
pub const ERROR_COLOR: u32 = 0xff4747;

/// String containing a mention of the bot, in the format `<@{id}>`.
static BOT_MENTION: OnceCell<String> = OnceCell::new();
/// String containing a mention of the bot as in a guild, in the format `<@!{id}>`.
static BOT_NICK_MENTION: OnceCell<String> = OnceCell::new();

/// Gets a string containing a mention of the bot, in the format `<@{id}>`.
pub fn bot_mention() -> &'static str {
	BOT_MENTION
		.get()
		.expect("Bot mention was not initialized")
		.as_str()
}

/// Gets a string containing a mention of the bot as in a guild, in the format `<@!{id}>`.
pub fn bot_nick_mention() -> &'static str {
	BOT_NICK_MENTION
		.get()
		.expect("Bot nick mention was not initialized")
		.as_str()
}

/// Sets up mention constants using the provided user ID.
pub fn init_mentions(bot_id: UserId) {
	let _ = BOT_MENTION.set(format!("<@{}>", bot_id));
	let _ = BOT_NICK_MENTION.set(format!("<@!{}>", bot_id));
}

/// Settings configured by the hoster.
static SETTINGS: OnceCell<Settings> = OnceCell::new();

/// Gets the settings configured by the hoster.
pub fn settings() -> &'static Settings {
	SETTINGS.get().expect("Settings were not initialized")
}

/// Initialize the bot's [`Settings`](Settings).
pub fn init_settings() {
	match Settings::new() {
		Ok(settings) => {
			let _ = SETTINGS.set(settings);
		}
		Err(e) => {
			panic!("Failed to parse settings: {}", e);
		}
	}
}
