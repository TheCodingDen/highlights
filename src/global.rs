// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Global constants, both compile-time and set at runtime.

use once_cell::sync::OnceCell;
use serenity::{model::id::UserId, CacheAndHttp};

use std::sync::Arc;

/// How many times to retry notifications after internal server errors from Discord.
pub const NOTIFICATION_RETRIES: u8 = 5;

/// Color of normal embeds (from help command and notifications).
pub const EMBED_COLOR: u32 = 0xefff47;
pub const ERROR_COLOR: u32 = 0xff4747;

/// String containing a mention of the bot, in the format `<@{id}>`.
static BOT_MENTION: OnceCell<String> = OnceCell::new();
/// String containing a mention of the bot as in a guild, in the format `<@!{id}>`.
static BOT_NICK_MENTION: OnceCell<String> = OnceCell::new();

static CACHE_HTTP: OnceCell<Arc<CacheAndHttp>> = OnceCell::new();

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

pub fn cache() -> Arc<CacheAndHttp> {
	CACHE_HTTP
		.get()
		.expect("Cache/HTTP was not initialized")
		.clone()
}

/// Sets up mention constants using the provided user ID.
pub fn init_mentions(bot_id: UserId) {
	let _ = BOT_MENTION.set(format!("<@{}>", bot_id));
	let _ = BOT_NICK_MENTION.set(format!("<@!{}>", bot_id));
}

pub fn init_cache(cache: Arc<CacheAndHttp>) {
	CACHE_HTTP
		.set(cache)
		.unwrap_or_else(|_| panic!("Cache already set"));
}
