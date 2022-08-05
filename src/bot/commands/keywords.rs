// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for adding, removing, and listing keywords.

use std::{collections::HashMap, fmt::Write};

use anyhow::{Context as _, Result};
use futures_util::{stream::FuturesUnordered, TryStreamExt};
use indoc::indoc;
use lazy_regex::regex;
use once_cell::sync::Lazy;
use serenity::{
	client::Context,
	http::error::ErrorResponse,
	model::{
		channel::{Channel, ChannelType, GuildChannel},
		id::{ChannelId, GuildId},
		interactions::application_command::ApplicationCommandInteraction as Command,
	},
	prelude::HttpError,
	Error as SerenityError,
};

use super::util::get_text_channels_in_guild;
use crate::{
	bot::{
		highlighting::warn_for_failed_dm,
		util::{respond_eph, success, user_can_read_channel},
	},
	db::{Ignore, Keyword, KeywordKind},
	settings::settings,
};

/// Add a keyword.
///
/// Usage: `/add <keyword> [channel]`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn add(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(ctx, command);
	let guild_id = require_guild!(&ctx, &command);
	let user_id = command.user.id;

	let keyword_count = Keyword::user_keyword_count(user_id).await?;

	if keyword_count >= settings().behavior.max_keywords as u64 {
		static MSG: Lazy<String, fn() -> String> = Lazy::new(|| {
			format!(
				"You can't create more than {} keywords!",
				settings().behavior.max_keywords
			)
		});

		return respond_eph(&ctx, &command, MSG.as_str()).await;
	}

	let keyword = command
		.data
		.options
		.get(0)
		.and_then(|o| o.value.as_ref())
		.context("No keyword to add provided")?
		.as_str()
		.context("Keyword provided was not a string")?
		.trim()
		.to_lowercase();

	if keyword.len() < 3 {
		return respond_eph(
			&ctx,
			&command,
			"❌ You can't highlight keywords shorter than 3 characters!",
		)
		.await;
	}

	if !is_valid_keyword(&keyword) {
		return respond_eph(&ctx, &command, "❌ You can't add that keyword!")
			.await;
	}

	let keyword = match command.data.resolved.channels.values().next() {
		Some(channel) => {
			let channel = ctx
				.cache
				.guild_channel(channel.id)
				.context("Channel for keyword to add not cached")?;
			let self_id = ctx.cache.current_user_id();
			match user_can_read_channel(&ctx, &channel, self_id).await {
				Ok(Some(true)) => Keyword {
					keyword,
					user_id,
					kind: KeywordKind::Channel(channel.id),
				},
				Ok(Some(false)) => {
					return respond_eph(
						&ctx,
						&command,
						format!("❌ I can't read <#{}>!", channel.id),
					)
					.await
				}
				Ok(None) => return Err(anyhow::anyhow!(
					"Self permissions not found in channel {} in guild {}",
					channel.id,
					guild_id
				)),
				Err(e) => return Err(e.context(
					"Failed to check for self permissions to read muted channel",
				)),
			}
		}
		None => Keyword {
			keyword,
			user_id,
			kind: KeywordKind::Guild(guild_id),
		},
	};

	if keyword.clone().exists().await? {
		return respond_eph(
			&ctx,
			&command,
			"❌ You already added that keyword!",
		)
		.await;
	}

	keyword.insert().await?;

	success(&ctx, &command).await?;

	if keyword_count == 0 {
		let dm_channel = command.user.create_dm_channel(&ctx).await?;

		match dm_channel
			.say(
				&ctx,
				indoc!(
					"
					Test message; if you can read this, \
					I can send you notifications successfully!"
				),
			)
			.await
		{
			Err(SerenityError::Http(err)) => match &*err {
				HttpError::UnsuccessfulRequest(ErrorResponse {
					error, ..
				}) if error.message == "Cannot send messages to this user" => {
					warn_for_failed_dm(&ctx, &command).await?;
				}

				_ => return Err(SerenityError::Http(err).into()),
			},
			Err(err) => return Err(err.into()),
			_ => {}
		}
	}

	Ok(())
}

fn is_valid_keyword(keyword: &str) -> bool {
	!regex!(r"<([@#&]|a?:)").is_match(keyword)
}

/// Remove a keyword.
///
/// Usage: `/remove <keyword> [channel]`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn remove(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(ctx, command);
	let guild_id = require_guild!(&ctx, &command);
	let user_id = command.user.id;

	let keyword = command
		.data
		.options
		.get(0)
		.and_then(|o| o.value.as_ref())
		.context("No keyword to add provided")?
		.as_str()
		.context("Keyword provided was not a string")?
		.trim()
		.to_lowercase();

	let keyword = match command.data.resolved.channels.values().next() {
		Some(channel) => Keyword {
			keyword,
			user_id,
			kind: KeywordKind::Channel(channel.id),
		},
		None => Keyword {
			keyword,
			user_id,
			kind: KeywordKind::Guild(guild_id),
		},
	};

	if !keyword.clone().exists().await? {
		return respond_eph(
			&ctx,
			&command,
			"❌ You haven't added that keyword!",
		)
		.await;
	}

	keyword.delete().await?;

	success(&ctx, &command).await
}

/// Add an ignored phrase.
///
/// Usage: `/ignore <phrase>`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn ignore(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(&ctx, command);
	let guild_id = require_guild!(&ctx, &command);

	let phrase = command
		.data
		.options
		.get(0)
		.and_then(|o| o.value.as_ref())
		.context("No phrase to ignore provided")?
		.as_str()
		.context("Phrase provided not string")?;

	if phrase.len() < 3 {
		return respond_eph(
			&ctx,
			&command,
			"❌ You can't ignore phrases shorter than 3 characters!",
		)
		.await;
	}

	let ignore = Ignore {
		user_id: command.user.id,
		guild_id,
		phrase: phrase.to_lowercase(),
	};

	if ignore.clone().exists().await? {
		return respond_eph(
			&ctx,
			&command,
			"❌ You already ignored that phrase!",
		)
		.await;
	}

	ignore.insert().await?;

	success(&ctx, &command).await
}

/// Remove an ignored phrase.
///
/// Usage: `/unignore <phrase>`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn unignore(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(&ctx, command);
	let guild_id = require_guild!(&ctx, &command);

	let phrase = command
		.data
		.options
		.get(0)
		.and_then(|o| o.value.as_ref())
		.context("No phrase to ignore provided")?
		.as_str()
		.context("Phrase provided not string")?;

	let ignore = Ignore {
		user_id: command.user.id,
		guild_id,
		phrase: phrase.to_lowercase(),
	};

	if !ignore.clone().exists().await? {
		return respond_eph(
			&ctx,
			&command,
			"❌ You haven't ignored that phrase!",
		)
		.await;
	}

	ignore.delete().await?;

	success(&ctx, &command).await
}

/// List ignored phrases in the current guild, or in all guilds when used in
/// DMs.
///
/// Usage: `/ignores`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn ignores(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(&ctx, command);

	match command.guild_id {
		Some(guild_id) => {
			let ignores = Ignore::user_guild_ignores(command.user.id, guild_id)
				.await?
				.into_iter()
				.map(|ignore| ignore.phrase)
				.collect::<Vec<_>>();

			if ignores.is_empty() {
				return respond_eph(
					&ctx,
					&command,
					"❌ You haven't ignored any phrases!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.context("Couldn't get guild to list ignores")?;

			let response = format!(
				"{}'s ignored phrases in {}:\n  - {}",
				command.user.name,
				guild_name,
				ignores.join("\n  - ")
			);

			respond_eph(&ctx, &command, response).await
		}
		None => {
			let ignores = Ignore::user_ignores(command.user.id).await?;

			if ignores.is_empty() {
				return respond_eph(
					&ctx,
					&command,
					"❌ You haven't ignored any phrases!",
				)
				.await;
			}

			let mut ignores_by_guild = HashMap::new();

			for ignore in ignores {
				ignores_by_guild
					.entry(ignore.guild_id)
					.or_insert_with(Vec::new)
					.push(ignore.phrase);
			}

			let mut response = String::new();

			for (guild_id, phrases) in ignores_by_guild {
				if !response.is_empty() {
					response.push_str("\n\n");
				}

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.context("Couldn't get guild to list ignores")?;

				write!(
					&mut response,
					"Your ignored phrases in {}:\n  – {}",
					guild_name,
					phrases.join("\n  – ")
				)
				.unwrap();
			}

			respond_eph(&ctx, &command, response).await
		}
	}
}

/// Remove keywords and ignores in a guild by ID.
///
/// Usage: `/remove-server <guild ID>`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn remove_server(
	ctx: Context,
	command: Command,
) -> Result<()> {
	check_opt_out!(&ctx, command);

	let arg = command
		.data
		.options
		.get(0)
		.and_then(|o| o.value.as_ref())
		.context("No guild ID to remove provided")?
		.as_str()
		.context("Guild ID to remove was not a string")?;

	let guild_id = match arg.parse() {
		Ok(id) => GuildId(id),
		Err(_) => {
			return respond_eph(&ctx, &command, "❌ Invalid server ID!").await
		}
	};

	let channels: Option<Vec<ChannelId>> =
		ctx.cache.guild_field(guild_id, |g| {
			g.channels
				.iter()
				.filter(|(_, channel)| {
					matches!(
						channel,
						Channel::Guild(GuildChannel {
							kind: ChannelType::Text,
							..
						})
					)
				})
				.map(|(&id, _)| id)
				.collect()
		});

	let guild_keywords_deleted =
		Keyword::delete_in_guild(command.user.id, guild_id).await?;

	let ignores_deleted =
		Ignore::delete_in_guild(command.user.id, guild_id).await?;

	let channel_keywords_deleted = match channels {
		Some(channels) => {
			let futures: FuturesUnordered<_> = channels
				.into_iter()
				.map(|channel| {
					Keyword::delete_in_channel(command.user.id, channel)
				})
				.collect();

			futures
				.try_fold(0, |acc, n| async move { Ok(acc + n) })
				.await?
		}
		None => 0,
	};

	if guild_keywords_deleted + ignores_deleted + channel_keywords_deleted == 0
	{
		respond_eph(
			&ctx,
			&command,
			"❌ You didn't have any keywords or ignores in that server!",
		)
		.await
	} else {
		success(&ctx, &command).await
	}
}

/// List keywords in the current guild, or in all guilds when used in DMs.
///
/// Usage: `/keywords`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn keywords(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(&ctx, command);

	match command.guild_id {
		Some(guild_id) => {
			let guild_keywords =
				Keyword::user_guild_keywords(command.user.id, guild_id)
					.await?
					.into_iter()
					.map(|keyword| keyword.keyword)
					.collect::<Vec<_>>();

			let guild_channels = get_text_channels_in_guild(&ctx, guild_id)?;

			let mut channel_keywords = HashMap::new();

			for keyword in
				Keyword::user_channel_keywords(command.user.id).await?
			{
				let channel_id = match keyword.kind {
					KeywordKind::Channel(id) => id,
					_ => {
						panic!("user_channel_keywords returned a guild keyword")
					}
				};

				if !guild_channels.contains_key(&channel_id) {
					continue;
				}

				channel_keywords
					.entry(channel_id)
					.or_insert_with(Vec::new)
					.push(keyword.keyword);
			}

			if guild_keywords.is_empty() && channel_keywords.is_empty() {
				return respond_eph(
					&ctx,
					&command,
					"❌ You haven't added any keywords yet!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.context("Couldn't get guild to list keywords")?;

			let mut response = String::with_capacity(45);

			if guild_keywords.is_empty() {
				write!(&mut response, "Your keywords in {}:", guild_name)
					.unwrap();
			} else {
				write!(
					&mut response,
					"Your keywords in {}:\n  – {}",
					guild_name,
					guild_keywords.join("\n  – ")
				)
				.unwrap();
			}

			for (channel_id, channel_keywords) in channel_keywords {
				response.push('\n');

				write!(
					&mut response,
					"  In <#{}>:\n    - {1}",
					channel_id,
					channel_keywords.join("\n    - "),
				)
				.unwrap();
			}

			respond_eph(&ctx, &command, response).await
		}
		None => {
			let keywords = Keyword::user_keywords(command.user.id).await?;

			if keywords.is_empty() {
				return respond_eph(
					&ctx,
					&command,
					"❌ You haven't added any keywords yet!",
				)
				.await;
			}

			let mut keywords_by_guild = HashMap::new();

			let mut unknown_channel_keywords = HashMap::new();

			for keyword in keywords {
				match keyword.kind {
					KeywordKind::Guild(guild_id) => {
						let guild_keywords = &mut keywords_by_guild
							.entry(guild_id)
							.or_insert_with(|| (Vec::new(), HashMap::new()))
							.0;

						guild_keywords.push(keyword.keyword);
					}
					KeywordKind::Channel(channel_id) => {
						let guild_id = ctx
							.cache
							.guild_channel_field(channel_id, |c| c.guild_id);

						match guild_id {
							Some(guild_id) => {
								keywords_by_guild
									.entry(guild_id)
									.or_insert_with(|| {
										(Vec::new(), HashMap::new())
									})
									.1
									.entry(channel_id)
									.or_insert_with(Vec::new)
									.push(keyword.keyword);
							}
							None => {
								unknown_channel_keywords
									.entry(channel_id)
									.or_insert_with(Vec::new)
									.push(keyword.keyword);
							}
						}
					}
				}
			}

			let mut response = String::new();

			for (guild_id, (guild_keywords, channel_keywords)) in
				keywords_by_guild
			{
				if !response.is_empty() {
					response.push_str("\n\n");
				}

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.unwrap_or_else(|| {
						format!("<Unknown server> ({})", guild_id)
					});

				if guild_keywords.is_empty() {
					write!(&mut response, "Your keywords in {}:", guild_name)
						.unwrap();
				} else {
					write!(
						&mut response,
						"Your keywords in {}:\n  – {}",
						guild_name,
						guild_keywords.join("\n  – ")
					)
					.unwrap();
				}

				for (channel_id, channel_keywords) in channel_keywords {
					response.push('\n');

					write!(
						&mut response,
						"  In <#{0}> ({0}):\n    - {1}",
						channel_id,
						channel_keywords.join("\n    - "),
					)
					.unwrap();
				}
			}

			respond_eph(&ctx, &command, response).await
		}
	}
}
