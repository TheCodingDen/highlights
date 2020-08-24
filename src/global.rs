// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::OnceCell;
use serenity::{
	client::Context,
	model::id::{ChannelId, UserId},
};

use std::{env, time::Duration};

pub const MAX_KEYWORDS: u32 = 100;

pub const PATIENCE_DURATION: Duration = Duration::from_secs(60 * 2);

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
	BOT_MENTION.set(format!("<@{}>", bot_id)).unwrap();
	BOT_NICK_MENTION.set(format!("<@!{}>", bot_id)).unwrap();
}

static LOG_CHANNEL_ID: OnceCell<ChannelId> = OnceCell::new();

pub fn log_channel_id() -> ChannelId {
	*LOG_CHANNEL_ID
		.get()
		.expect("Log channel id was not initialized")
}

pub async fn init_log_channel_id(ctx: &Context) {
	LOG_CHANNEL_ID
		.set(get_channel_for_owner_id(ctx).await)
		.unwrap();
}

async fn get_channel_for_owner_id(ctx: &Context) -> ChannelId {
	let owner_id = {
		const DEFAULT: u64 = 257711607096803328;

		env::var("HIGHLIGHTS_OWNER_ID")
			.ok()
			.and_then(|s| s.parse().ok())
			.unwrap_or(DEFAULT)
	};

	if let Some(u) = ctx.cache.user(owner_id).await {
		return u
			.create_dm_channel(ctx)
			.await
			.expect("Failed to open DM channel")
			.id;
	}

	if let Some(c) = ctx.cache.channel(owner_id).await {
		return c.id();
	}

	if let Ok(u) = ctx.http.get_user(owner_id).await {
		return u
			.create_dm_channel(ctx)
			.await
			.expect("Failed to open DM channel")
			.id;
	}

	ctx.http
		.get_channel(owner_id)
		.await
		.expect("Failed to find channel or user with OWNER_ID")
		.id()
}
