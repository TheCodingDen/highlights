// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros used by commands.

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	http::CacheHttp,
	model::{
		channel::{ChannelType, GuildChannel},
		id::{ChannelId, GuildId, UserId},
		prelude::User,
	},
};

use crate::{bot::util::user_can_read_channel, regex};
use std::{collections::HashMap, iter::FromIterator};

/// Requires the given message to have come from a guild channel.
///
/// Uses [`error`](crate::util::error) if the message did not come from a guild channel. Evaluates
/// to the guild's ID otherwise.
#[macro_export]
macro_rules! require_guild {
	($ctx:expr, $message:expr) => {{
		match $message.guild_id {
			None => {
				return $crate::bot::util::error(
					$ctx,
					$message,
					"You must run this command in a server!",
				)
				.await
			}
			Some(id) => id,
		}
	}};
}

/// Requires the given arguments to be non-empty.
///
/// Returns with [`question`](crate::util::question) if the arguments are empty.
#[macro_export]
macro_rules! require_nonempty_args {
	($args:expr, $ctx:expr, $message:expr) => {{
		if $args.is_empty() {
			return $crate::bot::util::question($ctx, $message).await;
		}
	}};
}

/// Requires the given arguments to be empty.
///
/// Returns with [`question`](crate::util::question) if the arguments are non-empty.
#[macro_export]
macro_rules! require_empty_args {
	($args:expr, $ctx:expr, $message:expr) => {{
		if !$args.is_empty() {
			return $crate::bot::util::question($ctx, $message).await;
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

/// Results of getting users from a list of args.
#[derive(Debug, Default)]
pub struct UsersFromArgs<'args> {
	/// Users that were successfully found by ID.
	pub found: Vec<User>,
	/// IDs that were not resolved to any users.
	pub not_found: Vec<u64>,
	/// Arguments that were not valid IDs or mentions.
	pub invalid: Vec<&'args str>,
}

/// Gets users from arguments.
///
/// `args` is split by whitespace, and each split substring is checked for a user ID or user mention.
/// `ctx` is used to fetch users by this ID.
#[allow(clippy::needless_lifetimes)]
pub async fn get_users_from_args<'args>(
	ctx: &Context,
	args: &'args str,
) -> UsersFromArgs<'args> {
	let mut results = UsersFromArgs::default();

	for word in args.split_whitespace() {
		match regex!(r"([0-9]{16,20})|<@!?([0-9]{16,20})>").captures(word) {
			Some(captures) => {
				let id = captures
					.get(1)
					.or_else(|| captures.get(2))
					.unwrap()
					.as_str()
					.parse()
					.unwrap();

				match ctx.http.get_user(id).await {
					Ok(user) => results.found.push(user),
					Err(_) => results.not_found.push(id),
				}
			}
			None => results.invalid.push(word),
		}
	}

	results
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

/// Gets channels from arguments, filtering channels that can't be read by the author or the bot.
///
/// First gets all channels from the arguments, then checks the bots' and the provided user's
/// permissions in each to sort them into a `ReadableChannelsFromArgs`.
pub async fn get_readable_channels_from_args<'args, 'c>(
	ctx: &impl CacheHttp,
	author_id: UserId,
	channels: &'c HashMap<ChannelId, GuildChannel>,
	args: &'args str,
) -> Result<ReadableChannelsFromArgs<'args, 'c>> {
	let all_channels = get_channels_from_args(channels, args);

	let mut result = ReadableChannelsFromArgs {
		not_found: all_channels.not_found,
		..Default::default()
	};

	let self_id = ctx.cache().unwrap().current_user_id().await;

	for (channel, arg) in all_channels.found {
		let user_can_read = user_can_read_channel(ctx, channel, author_id)
			.await?
			.context("No permissions for user to get readable channels")?;

		let self_can_read = user_can_read_channel(ctx, channel, self_id)
			.await?
			.context("No permissions for self to get readable channels")?;

		if !user_can_read {
			result.user_cant_read.push((channel, arg));
		} else if !self_can_read {
			result.self_cant_read.push(channel);
		} else {
			result.found.push(channel);
		}
	}

	Ok(result)
}

/// Parses whitespace-separated IDs from the provided arguments.
///
/// Each element of the returned `Vec` is `Ok((id, arg))` if `arg` was a valid ID, and `Err(arg)`
/// if `arg` was an invalid ID.
pub fn get_ids_from_args(args: &str) -> Vec<Result<(ChannelId, &str), &str>> {
	args.split_whitespace()
		.map(|arg| arg.parse().map(|id| (ChannelId(id), arg)).map_err(|_| arg))
		.collect()
}

/// Gets channels from the provided map by whitespace-separated arguments in the provided string.
fn get_channels_from_args<'args, 'c>(
	channels: &'c HashMap<ChannelId, GuildChannel>,
	args: &'args str,
) -> ChannelsFromArgs<'args, 'c> {
	args.split_whitespace()
		.map(|arg| get_channel_from_arg(channels, arg))
		.collect()
}

/// Gets a channel from an argument.
///
/// If `arg` is an ID, and a channel with that ID exists in the provided map, `Ok(channel, arg)`
/// is returned.
///
/// If `arg` is a mention, and a channel with the ID of that mention exists in the provided map,
/// `Ok(channel, arg)` is returned.
///
/// If a channel that has a name matching `arg` exists in the provided map, `Ok(channel, arg)` is
/// returned.
///
/// Otherwise, `Err(arg)` is returned.
fn get_channel_from_arg<'arg, 'c>(
	channels: &'c HashMap<ChannelId, GuildChannel>,
	arg: &'arg str,
) -> Result<(&'c GuildChannel, &'arg str), &'arg str> {
	if let Ok(id) = arg.parse::<u64>() {
		return match channels.get(&ChannelId(id)) {
			Some(c) => Ok((c, arg)),
			None => Err(arg),
		};
	}

	if let Some(id) = arg
		.strip_prefix("<#")
		.and_then(|arg| arg.strip_suffix('>'))
		.and_then(|arg| arg.parse::<u64>().ok())
	{
		return match channels.get(&ChannelId(id)) {
			Some(c) => Ok((c, arg)),
			None => Err(arg),
		};
	}

	let mut iter = channels
		.iter()
		.map(|(_, channel)| channel)
		.filter(|channel| channel.name.as_str().eq_ignore_ascii_case(arg));

	if let Some(first) = iter.next() {
		if iter.next().is_none() {
			return Ok((first, arg));
		}
	}

	Err(arg)
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
