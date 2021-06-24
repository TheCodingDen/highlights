// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for adding, removing, and listing channel mutes.

use super::util::{
	get_ids_from_args, get_readable_channels_from_args,
	get_text_channels_in_guild,
};

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	model::channel::{ChannelType, Message},
};

use std::{collections::HashMap, fmt::Write};

use crate::{
	bot::{responses::insert_command_response, util::error},
	db::Mute,
	monitoring::Timer,
};

/// Mute a channel.
///
/// Usage: `@Highlights mute <whitespace-separated channel IDs or mentions>`
pub async fn mute(ctx: &Context, message: &Message, args: &str) -> Result<()> {
	let _timer = Timer::command("mute");
	let guild_id = require_guild!(ctx, message);

	require_nonempty_args!(args, ctx, message);

	let channels = get_text_channels_in_guild(ctx, guild_id).await?;

	let channel_args = get_readable_channels_from_args(
		ctx,
		message.author.id,
		&channels,
		args,
	)
	.await?;

	let mut not_found = channel_args.not_found;
	not_found
		.extend(channel_args.user_cant_read.into_iter().map(|(_, arg)| arg));

	let cant_mute = channel_args
		.self_cant_read
		.into_iter()
		.map(|c| format!("<#{}>", c.id))
		.collect::<Vec<_>>();

	let mut muted = vec![];
	let mut already_muted = vec![];

	for channel in channel_args.found {
		let mute = Mute {
			user_id: message.author.id,
			channel_id: channel.id,
		};

		if mute.clone().exists().await? {
			already_muted.push(format!("<#{}>", channel.id));
		} else {
			muted.push(format!("<#{}>", channel.id));
			mute.insert().await?;
		}
	}

	let mut msg = String::with_capacity(45);

	if !muted.is_empty() {
		msg.push_str("Muted channels: ");
		msg.push_str(&muted.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !already_muted.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Channels already muted: ");
		msg.push_str(&already_muted.join(", "));

		message.react(ctx, '❌').await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find channels: ");
		msg.push_str(&not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	if !cant_mute.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Unable to mute channels: ");
		msg.push_str(&cant_mute.join(", "));

		if already_muted.is_empty() {
			message.react(ctx, '❌').await?;
		}
	}

	let response = message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	insert_command_response(ctx, message.id, response.id).await;

	Ok(())
}

/// Unmute a channel.
///
/// Usage: `@Highlights unmute <whitespace-separated channel IDs or mentions>`
pub async fn unmute(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("unmute");
	require_nonempty_args!(args, ctx, message);

	let channels = match message.guild_id {
		Some(guild_id) => {
			ctx.cache.guild_channels(guild_id).await.map(|channels| {
				channels
					.into_iter()
					.filter(|(_, channel)| channel.kind == ChannelType::Text)
					.collect()
			})
		}
		None => None,
	};

	let user_id = message.author.id;

	let mut unmuted = vec![];
	let mut not_muted = vec![];
	let mut not_found = vec![];

	match channels.as_ref() {
		Some(channels) => {
			let channel_args = get_readable_channels_from_args(
				ctx,
				message.author.id,
				channels,
				args,
			)
			.await?;

			not_found = channel_args.not_found;

			for (user_unreadable, arg) in channel_args.user_cant_read {
				let mute = Mute {
					user_id,
					channel_id: user_unreadable.id,
				};

				if !mute.clone().exists().await? {
					not_found.push(arg);
				} else {
					unmuted.push(format!("<#{0}> ({0})", user_unreadable.id));
					mute.delete().await?;
				}
			}

			for self_unreadable in channel_args
				.found
				.into_iter()
				.chain(channel_args.self_cant_read)
			{
				let mute = Mute {
					user_id,
					channel_id: self_unreadable.id,
				};

				if !mute.clone().exists().await? {
					not_muted.push(format!("<#{0}>", self_unreadable.id));
				} else {
					unmuted.push(format!("<#{0}>", self_unreadable.id));
					mute.delete().await?;
				}
			}
		}
		None => {
			for result in get_ids_from_args(args) {
				match result {
					Ok((channel_id, arg)) => {
						let mute = Mute {
							user_id,
							channel_id,
						};

						if !mute.clone().exists().await? {
							not_found.push(arg);
						} else {
							unmuted.push(format!("<#{0}> ({0})", channel_id));
							mute.delete().await?;
						}
					}
					Err(arg) => {
						not_found.push(arg);
					}
				}
			}
		}
	}

	let mut msg = String::with_capacity(50);

	if !unmuted.is_empty() {
		msg.push_str("Unmuted channels: ");
		msg.push_str(&unmuted.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !not_muted.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Channels weren't muted: ");
		msg.push_str(&not_muted.join(", "));

		message.react(ctx, '❌').await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find channels: ");
		msg.push_str(&not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	let response = message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	insert_command_response(ctx, message.id, response.id).await;

	Ok(())
}

/// List muted channels in the current guild, or all guilds when used in DMs.
///
/// Usage: `@Highlights mutes`
pub async fn mutes(ctx: &Context, message: &Message, args: &str) -> Result<()> {
	let _timer = Timer::command("mutes");
	require_empty_args!(args, ctx, message);
	match message.guild_id {
		Some(guild_id) => {
			let channels = ctx
				.cache
				.guild_channels(guild_id)
				.await
				.context("Couldn't get guild channels to list mutes")?;

			let mutes = Mute::user_mutes(message.author.id)
				.await?
				.into_iter()
				.filter(|mute| channels.contains_key(&mute.channel_id))
				.map(|mute| format!("<#{}>", mute.channel_id))
				.collect::<Vec<_>>();

			if mutes.is_empty() {
				return error(ctx, message, "You haven't muted any channels!")
					.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.context("Couldn't get guild to list mutes")?;

			let response = format!(
				"{}'s muted channels in {}:\n  - {}",
				message.author.name,
				guild_name,
				mutes.join("\n  - ")
			);

			let response = message
				.channel_id
				.send_message(ctx, |m| {
					m.content(response).allowed_mentions(|m| m.empty_parse())
				})
				.await?;

			insert_command_response(ctx, message.id, response.id).await;
		}
		None => {
			let mutes = Mute::user_mutes(message.author.id).await?;

			if mutes.is_empty() {
				return error(ctx, message, "You haven't muted any channels!")
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

			let response = message
				.channel_id
				.send_message(ctx, |m| {
					m.content(response).allowed_mentions(|m| m.empty_parse())
				})
				.await?;

			insert_command_response(ctx, message.id, response.id).await;
		}
	}

	Ok(())
}
