// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros used by commands.

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	model::{
		channel::{ChannelType, GuildChannel},
		id::{ChannelId, GuildId},
	},
};

/// Requires the given command to have come from a guild channel.
///
/// Uses [`respond_eph`](crate::bot::util::respond_eph) if the command did not \
/// come from a guild channel. Evaluates to the guild's ID otherwise.
#[macro_export]
macro_rules! require_guild {
	($ctx:expr, $command:expr) => {{
		#[allow(clippy::needless_borrow)]
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

/// Requires the author of a given command to not be opted out.
///
/// Uses [`respond_eph`](crate::bot::util::respond_eph) to display an error if
/// the user is opted out.
#[macro_export]
macro_rules! check_opt_out {
	($ctx:expr, $command:expr) => {{
		let opt_out = $crate::db::OptOut {
			user_id: $command.user.id,
		};

		if opt_out.exists().await? {
			return $crate::bot::util::respond_eph(
				&$ctx,
				&$command,
				"❌ You can't use this command after opting out!",
			)
			.await;
		}
	}};
}

/// Requires the current bot member to have permission to send embeds.
///
/// Uses [`respond_eph`](crate::bot::util::respond_eph) if the current member does not have permission to send
/// embeds. Does nothing if used on a command in a DM channel.
#[macro_export]
macro_rules! require_embed_perms {
	($ctx:expr, $command:expr) => {
		#[allow(clippy::needless_borrow)]
		if $command.guild_id.is_some() {
			use ::anyhow::Context as _;
			let self_id = $ctx.cache.current_user_id();

			let channel = $ctx
				.cache
				.guild_channel($command.channel_id)
				.context("Nonexistent guild channel")?;

			let permissions = channel
				.permissions_for_user($ctx, self_id)
				.context("Failed to get permissions for self")?;

			if !permissions.embed_links() {
				$crate::bot::util::respond_eph(
					$ctx,
					$command,
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
pub(crate) async fn get_text_channels_in_guild(
	ctx: &Context,
	guild_id: GuildId,
) -> Result<HashMap<ChannelId, GuildChannel>> {
	let channels = ctx
		.cache
		.guild_channels(guild_id)
		.context("Couldn't get guild to get channels")?;
	let channels = channels
		.into_iter()
		.filter(|(_, channel)| channel.kind == ChannelType::Text)
		.collect();

	Ok(channels)
}
