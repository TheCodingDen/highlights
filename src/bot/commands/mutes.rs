// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for adding, removing, and listing channel mutes.

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	model::interactions::application_command::ApplicationCommandInteraction as Command,
};

use std::{collections::HashMap, fmt::Write};

use crate::{
	bot::util::{respond_eph, user_can_read_channel},
	db::Mute,
	monitoring::Timer,
};

/// Mute a channel.
///
/// Usage: `/mute <channel>`
pub async fn mute(ctx: &Context, mut command: Command) -> Result<()> {
	let _timer = Timer::command("mute");
	let guild_id = require_guild!(ctx, &command);

	let channel_id = command
		.data
		.resolved
		.channels
		.drain()
		.next()
		.map(|(id, _)| id)
		.context("No channel to mute provided")?;

	let channel = ctx
		.cache
		.guild_channel(channel_id)
		.await
		.context("Failed to get guild channel to mute")?;

	match user_can_read_channel(
		ctx,
		&channel,
		ctx.cache.current_user_id().await,
	)
	.await
	{
		Ok(Some(true)) => {
			let mute = Mute {
				user_id: command.user.id,
				channel_id,
			};

			if mute.clone().exists().await? {
				respond_eph(
					ctx,
					&command,
					format!("❌ You've already muted <#{}>!", channel_id),
				)
				.await
			} else {
				mute.insert().await?;
				respond_eph(
					ctx,
					&command,
					format!("✅ Muted <#{}>", channel_id),
				)
				.await
			}
		}
		Ok(Some(false)) => {
			respond_eph(
				ctx,
				&command,
				format!("❌ I can't read <#{}>!", channel_id),
			)
			.await
		}
		Ok(None) => Err(anyhow::anyhow!(
			"Self permissions not found in channel {} in guild {}",
			channel_id,
			guild_id
		)),
		Err(e) => Err(e.context(
			"Failed to check for self permissions to read muted channel",
		)),
	}
}

/// Unmute a channel.
///
/// Usage: `/unmute <channel>`
pub async fn unmute(ctx: &Context, mut command: Command) -> Result<()> {
	let _timer = Timer::command("unmute");

	let channel_id = command
		.data
		.resolved
		.channels
		.drain()
		.next()
		.map(|(id, _)| id)
		.context("No channel to mute provided")?;

	let mute = Mute {
		user_id: command.user.id,
		channel_id,
	};

	if !mute.clone().exists().await? {
		respond_eph(
			ctx,
			&command,
			format!("❌ You haven't muted <#{}>!", channel_id),
		)
		.await
	} else {
		mute.delete().await?;
		respond_eph(ctx, &command, format!("✅ Unmuted <#{}>", channel_id))
			.await
	}
}

/// List muted channels in the current guild.
///
/// Usage: `/mutes`
pub async fn mutes(ctx: &Context, command: Command) -> Result<()> {
	let _timer = Timer::command("mutes");
	match command.guild_id {
		Some(guild_id) => {
			let channels = ctx
				.cache
				.guild_channels(guild_id)
				.await
				.context("Couldn't get guild channels to list mutes")?;

			let mutes = Mute::user_mutes(command.user.id)
				.await?
				.into_iter()
				.filter(|mute| channels.contains_key(&mute.channel_id))
				.map(|mute| format!("<#{}>", mute.channel_id))
				.collect::<Vec<_>>();

			if mutes.is_empty() {
				return respond_eph(
					ctx,
					&command,
					"❌ You haven't muted any channels!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.context("Couldn't get guild to list mutes")?;

			let response = format!(
				"Your muted channels in {}:\n  - {}",
				guild_name,
				mutes.join("\n  - ")
			);

			respond_eph(ctx, &command, response).await
		}
		None => {
			let mutes = Mute::user_mutes(command.user.id).await?;

			if mutes.is_empty() {
				return respond_eph(
					ctx,
					&command,
					"❌ You haven't muted any channels!",
				)
				.await;
			}

			let mut mutes_by_guild = HashMap::new();
			let mut not_found = Vec::new();

			for mute in mutes {
				let channel =
					match ctx.cache.guild_channel(mute.channel_id).await {
						Some(channel) => channel,
						None => {
							not_found
								.push(format!("<#{0}> ({0})", mute.channel_id));
							continue;
						}
					};

				mutes_by_guild
					.entry(channel.guild_id)
					.or_insert_with(Vec::new)
					.push(format!("<#{}>", mute.channel_id));
			}

			let mut response = String::new();

			for (guild_id, channel_ids) in mutes_by_guild {
				if !response.is_empty() {
					response.push_str("\n\n");
				}

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.await
					.context("Couldn't get guild to list mutes")?;

				write!(
					&mut response,
					"Your muted channels in {}:\n  – {}",
					guild_name,
					channel_ids.join("\n  – ")
				)
				.unwrap();
			}

			if !not_found.is_empty() {
				write!(
					&mut response,
					"\n\nCouldn't find (deleted?) muted channels:\n  – {}",
					not_found.join("\n  – ")
				)
				.unwrap();
			}

			respond_eph(ctx, &command, response).await
		}
	}
}
