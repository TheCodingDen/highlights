// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::OnceCell;
use serenity::model::id::UserId;

use std::time::Duration;

pub const MAX_KEYWORDS: u32 = 100;

pub const PATIENCE_DURATION: Duration = Duration::from_secs(60 * 2);

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
