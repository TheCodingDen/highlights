// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros used by commands.

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	model::{
		channel::{ChannelType, GuildChannel},
		id::{ChannelId, GuildId},
	},
};

use std::{collections::HashMap, iter::FromIterator};

/// Requires the given message to have come from a guild channel.
///
/// Uses [`error`](crate::util::error) if the message did not come from a guild channel. Evaluates
/// to the guild's ID otherwise.
#[macro_export]
macro_rules! require_guild {
	($ctx:expr, $command:expr) => {{
		match $command.guild_id {
			None => {
				return $crate::bot::util::respond_eph(
					$ctx,
					$command,
					"❌ You must run this command in a server!",
				)
				.await
			}
			Some(id) => id,
		}
	}};
}

/// Requires the current bot member to have permission to send embeds.
///
/// Uses [`error`](crate::util::error) if the current member does not have permission to send
/// embeds. Does nothing if used on a message in a DM channel.
#[macro_export]
macro_rules! require_embed_perms {
	($ctx:expr, $command:expr) => {
		if $command.guild_id.is_some() {
			use ::anyhow::Context as _;
			let self_id = $ctx.cache.current_user_id().await;

			let channel = $ctx
				.cache
				.guild_channel($command.channel_id)
				.await
				.context("Nonexistent guild channel")?;

			let permissions = channel
				.permissions_for_user($ctx, self_id)
				.await
				.context("Failed to get permissions for self")?;

			if !permissions.embed_links() {
				$crate::bot::util::respond_eph(
					&$ctx,
					&$command,
					"Sorry, I need permission to embed links to use that \
					command 😔",
				)
				.await
				.context("Failed to send missing embed permission message")?;

				return Ok(());
			}
		}
	};
}

/// Convenience function to get a map of all cached text channels in the given guild.
pub async fn get_text_channels_in_guild(
	ctx: &Context,
	guild_id: GuildId,
) -> Result<HashMap<ChannelId, GuildChannel>> {
	let channels = ctx
		.cache
		.guild_channels(guild_id)
		.await
		.context("Couldn't get guild to get channels")?;
	let channels = channels
		.into_iter()
		.filter(|(_, channel)| channel.kind == ChannelType::Text)
		.collect();

	Ok(channels)
}

/// Channels from a list of arguments.
#[derive(Debug, Default)]
struct ChannelsFromArgs<'args, 'c> {
	/// Arguments that couldn't be resolved to channels.
	not_found: Vec<&'args str>,
	/// Channels and the arguments used to find them.
	found: Vec<(&'c GuildChannel, &'args str)>,
}

impl<'args, 'c> FromIterator<Result<(&'c GuildChannel, &'args str), &'args str>>
	for ChannelsFromArgs<'args, 'c>
{
	fn from_iter<
		T: IntoIterator<Item = Result<(&'c GuildChannel, &'args str), &'args str>>,
	>(
		iter: T,
	) -> Self {
		let mut result = Self::default();
		iter.into_iter().for_each(|res| match res {
			Ok(c) => result.found.push(c),
			Err(arg) => result.not_found.push(arg),
		});
		result
	}
}

/// Readable channels from a list of arguments.
#[derive(Debug, Default)]
pub struct ReadableChannelsFromArgs<'args, 'c> {
	/// Arguments that couldn't be resolved to channels.
	pub not_found: Vec<&'args str>,
	/// Channels readable by both the user and the bot.
	pub found: Vec<&'c GuildChannel>,
	/// Channels not readable by the user, and the argument provided to find them.
	pub user_cant_read: Vec<(&'c GuildChannel, &'args str)>,
	/// Channels readable by the user, but not by the bot.
	pub self_cant_read: Vec<&'c GuildChannel>,
}
