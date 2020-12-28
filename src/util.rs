// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::Lazy;
use regex::Regex;
use serenity::{
	client::Context,
	model::{
		channel::{GuildChannel, Message},
		guild::{Guild, PartialGuild},
		id::UserId,
	},
};

use std::fmt::Display;

use crate::Error;

#[macro_export]
macro_rules! log_discord_error {
	(in $channel_id:expr, by $user_id:expr, $error:expr) => {
		log::error!(
			"Error in <#{0}> ({0}) by <@{1}> ({1}): {2}\n{2:?}",
			$channel_id,
			$user_id,
			$error
		);
	};
}

#[macro_export]
macro_rules! regex {
	($re:literal $(,)?) => {{
		static RE: once_cell::sync::OnceCell<regex::Regex> =
			once_cell::sync::OnceCell::new();
		RE.get_or_init(|| regex::Regex::new($re).unwrap())
		}};
}

pub static MD_SYMBOL_REGEX: Lazy<Regex, fn() -> Regex> =
	Lazy::new(|| Regex::new(r"[_*()\[\]~`]").unwrap());

pub async fn success(ctx: &Context, message: &Message) -> Result<(), Error> {
	message.react(ctx, '✅').await?;

	Ok(())
}

pub async fn question(ctx: &Context, message: &Message) -> Result<(), Error> {
	message.react(ctx, '❓').await?;

	Ok(())
}

pub async fn error<S: Display>(
	ctx: &Context,
	message: &Message,
	response: S,
) -> Result<(), Error> {
	let _ = message.react(ctx, '❌').await;

	message
		.channel_id
		.send_message(ctx, |m| {
			m.content(response).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	Ok(())
}

pub async fn user_can_read_channel(
	ctx: &Context,
	channel: &GuildChannel,
	user_id: UserId,
) -> Result<bool, Error> {
	enum MaybePartialGuild {
		Partial(PartialGuild),
		FullGuild(Guild),
	}

	use MaybePartialGuild::*;

	let guild = match ctx.cache.guild(channel.guild_id).await {
		Some(g) => FullGuild(g),
		None => Partial(ctx.http.get_guild(channel.guild_id.0).await?),
	};

	let member = match &guild {
		FullGuild(g) => g.member(ctx, user_id).await?,
		Partial(g) => g.member(ctx, user_id).await?,
	};

	let permissions = match &guild {
		FullGuild(g) => g.user_permissions_in(&channel, &member)?,
		Partial(g) => g.user_permissions_in(&channel, &member)?,
	};

	Ok(permissions.read_messages())
}
