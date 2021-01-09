// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Commands for adding, removing, and listing keywords.

use anyhow::{Context as _, Result};
use indoc::indoc;
use once_cell::sync::Lazy;
use regex::Regex;
use serenity::{
	client::Context,
	http::error::ErrorResponse,
	model::{
		channel::Message,
		id::{ChannelId, GuildId},
	},
	prelude::HttpError,
	Error as SerenityError,
};

use std::{collections::HashMap, convert::TryInto, fmt::Write};

use super::util::{
	get_readable_channels_from_args, get_text_channels_in_guild,
};
use crate::{
	db::{Ignore, Keyword, KeywordKind},
	global::settings,
	monitoring::Timer,
	util::{error, success, MD_SYMBOL_REGEX},
};

/// Pattern for channel-specific keywords.
///
/// Matches text such as `"foo" in bar baz`.
static CHANNEL_KEYWORD_REGEX: Lazy<Regex, fn() -> Regex> = Lazy::new(|| {
	Regex::new(r#"^"((?:\\"|[^"])*)" (?:in|from) ((?:\S+(?:$| ))+)"#).unwrap()
});

/// Add a keyword.
///
/// Usage:
/// - `@Highlights add <keyword>`
/// - `@Highlights add "<keyword>" in <space-separated channel names, mentions, or IDs>`
pub async fn add(ctx: &Context, message: &Message, args: &str) -> Result<()> {
	let _timer = Timer::command("add");
	let guild_id = require_guild!(ctx, message);

	require_nonempty_args!(args, ctx, message);

	{
		let keyword_count =
			Keyword::user_keyword_count(message.author.id).await?;

		if keyword_count >= settings().behavior.max_keywords {
			static MSG: Lazy<String, fn() -> String> = Lazy::new(|| {
				format!(
					"You can't create more than {} keywords!",
					settings().behavior.max_keywords
				)
			});

			return error(ctx, message, MSG.as_str()).await;
		}

		if keyword_count == 0 {
			let dm_channel = message.author.create_dm_channel(ctx).await?;

			match dm_channel
				.say(
					ctx,
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
						error,
						..
					}) if error.message
						== "Cannot send messages to this user" =>
					{
						message
							.reply(
								ctx,
								indoc!(
									"
									⚠️ I failed to DM you to make sure I \
									can notify you of your highlighted \
									keywords. Make sure you have DMs enabled \
									in at least one server that we share.",
								),
							)
							.await?;
					}

					_ => Err(SerenityError::Http(err))?,
				},
				Err(err) => Err(err)?,
				_ => {}
			}
		}
	}

	match CHANNEL_KEYWORD_REGEX.captures(args) {
		Some(captures) => {
			let keyword = captures
				.get(1)
				.context("Captures didn't contain keyword")?
				.as_str();
			let channel = captures
				.get(2)
				.context("Captures didn't contain channel")?
				.as_str();

			add_channel_keyword(ctx, message, guild_id, keyword, channel).await
		}
		None => add_guild_keyword(ctx, message, guild_id, args).await,
	}
}

/// Add a guild-wide keyword.
async fn add_guild_keyword(
	ctx: &Context,
	message: &Message,
	guild_id: GuildId,
	args: &str,
) -> Result<()> {
	if args.len() < 3 {
		return error(
			ctx,
			message,
			"You can't highlight keywords shorter than 3 characters!",
		)
		.await;
	}

	let keyword = Keyword {
		keyword: args.to_lowercase(),
		user_id: message.author.id.0.try_into().unwrap(),
		kind: KeywordKind::Guild(guild_id.0.try_into().unwrap()),
	};

	if keyword.clone().exists().await? {
		return error(ctx, message, "You already added that keyword!").await;
	}

	keyword.insert().await?;

	success(ctx, message).await
}

/// Add a channel-specific keyword.
async fn add_channel_keyword(
	ctx: &Context,
	message: &Message,
	guild_id: GuildId,
	keyword: &str,
	channels: &str,
) -> Result<()> {
	if keyword.len() < 3 {
		return error(
			ctx,
			message,
			"You can't highlight keywords shorter than 3 characters!",
		)
		.await;
	}

	let guild_channels = get_text_channels_in_guild(ctx, guild_id).await?;

	let user_id = message.author.id;

	let mut channel_args = get_readable_channels_from_args(
		ctx,
		user_id,
		&guild_channels,
		channels,
	)
	.await?;

	channel_args
		.not_found
		.extend(channel_args.user_cant_read.drain(..).map(|(_, arg)| arg));

	let cant_add = channel_args
		.self_cant_read
		.into_iter()
		.map(|c| format!("<#{}>", c.id))
		.collect::<Vec<_>>();

	let mut added = vec![];
	let mut already_added = vec![];

	let user_id = user_id.0.try_into().unwrap();

	for channel in channel_args.found {
		let keyword = Keyword {
			keyword: keyword.to_lowercase(),
			user_id,
			kind: KeywordKind::Channel(channel.id.0.try_into().unwrap()),
		};

		if keyword.clone().exists().await? {
			already_added.push(format!("<#{}>", channel.id));
		} else {
			added.push(format!("<#{}>", channel.id));
			keyword.insert().await?;
		}
	}

	let mut msg = String::with_capacity(45);

	let keyword = MD_SYMBOL_REGEX.replace_all(keyword, r"\$0");

	if !added.is_empty() {
		write!(
			&mut msg,
			"Added {} in channels: {}",
			keyword,
			added.join(", ")
		)
		.unwrap();

		message.react(ctx, '✅').await?;
	}

	if !already_added.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		write!(
			&mut msg,
			"Already added {} in channels: {}",
			keyword,
			already_added.join(", ")
		)
		.unwrap();

		message.react(ctx, '❌').await?;
	}

	if !channel_args.not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find channels: ");
		msg.push_str(&channel_args.not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	if !cant_add.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Unable to add keywords in channels: ");
		msg.push_str(&cant_add.join(", "));

		if already_added.is_empty() {
			message.react(ctx, '❌').await?;
		}
	}

	message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	Ok(())
}

/// Remove a keyword.
///
/// Usage:
/// - `@Highlights remove <keyword>`
/// - `@Highlights remove "<keyword>" from <space-separated channel names, mentions, or IDs>`
pub async fn remove(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("remove");
	let guild_id = require_guild!(ctx, message);

	require_nonempty_args!(args, ctx, message);

	match CHANNEL_KEYWORD_REGEX.captures(args) {
		Some(captures) => {
			let keyword = captures
				.get(1)
				.context("Captures didn't contain keyword")?
				.as_str();
			let channel = captures
				.get(2)
				.context("Captures didn't contain channel")?
				.as_str();

			remove_channel_keyword(ctx, message, guild_id, keyword, channel)
				.await
		}
		None => remove_guild_keyword(ctx, message, guild_id, args).await,
	}
}

/// Remove a guild-wide keyword.
async fn remove_guild_keyword(
	ctx: &Context,
	message: &Message,
	guild_id: GuildId,
	args: &str,
) -> Result<()> {
	let keyword = Keyword {
		keyword: args.to_lowercase(),
		user_id: message.author.id.0.try_into().unwrap(),
		kind: KeywordKind::Guild(guild_id.0.try_into().unwrap()),
	};

	if !keyword.clone().exists().await? {
		return error(ctx, message, "You haven't added that keyword!").await;
	}

	keyword.delete().await?;

	success(ctx, message).await
}

/// Remove a channel-specific keyword.
async fn remove_channel_keyword(
	ctx: &Context,
	message: &Message,
	guild_id: GuildId,
	keyword: &str,
	channels: &str,
) -> Result<()> {
	let guild_channels = get_text_channels_in_guild(ctx, guild_id).await?;

	let user_id = message.author.id;

	let channel_args = get_readable_channels_from_args(
		ctx,
		user_id,
		&guild_channels,
		channels,
	)
	.await?;

	let keyword = keyword.to_lowercase();

	let mut removed = vec![];
	let mut not_added = vec![];
	let mut not_found = channel_args.not_found;

	let user_id = user_id.0.try_into().unwrap();

	for channel in channel_args.found {
		let keyword = Keyword {
			keyword: keyword.to_owned(),
			user_id,
			kind: KeywordKind::Channel(channel.id.0.try_into().unwrap()),
		};

		if !keyword.clone().exists().await? {
			not_added.push(format!("<#{}>", channel.id));
		} else {
			removed.push(format!("<#{}>", channel.id));
			keyword.delete().await?;
		}
	}

	for (user_unreadable, arg) in channel_args.user_cant_read {
		let keyword = Keyword {
			keyword: keyword.clone(),
			user_id,
			kind: KeywordKind::Channel(
				user_unreadable.id.0.try_into().unwrap(),
			),
		};

		if !keyword.clone().exists().await? {
			not_found.push(arg);
		} else {
			removed.push(format!("<#{0}> ({0})", user_unreadable.id));
			keyword.delete().await?;
		}
	}

	for self_unreadable in channel_args.self_cant_read {
		let keyword = Keyword {
			keyword: keyword.clone(),
			user_id,
			kind: KeywordKind::Channel(
				self_unreadable.id.0.try_into().unwrap(),
			),
		};

		if !keyword.clone().exists().await? {
			not_added.push(format!("<#{0}>", self_unreadable.id));
		} else {
			removed.push(format!("<#{0}>", self_unreadable.id));
			keyword.delete().await?;
		}
	}

	let mut msg = String::with_capacity(45);

	let keyword = MD_SYMBOL_REGEX.replace_all(&keyword, r"\$0");

	if !removed.is_empty() {
		write!(
			&mut msg,
			"Removed {} from channels: {}",
			keyword,
			removed.join(", ")
		)
		.unwrap();

		message.react(ctx, '✅').await?;
	}

	if !not_added.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		write!(
			&mut msg,
			"{} wasn't added to channels: {}",
			keyword,
			not_added.join(", ")
		)
		.unwrap();

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

	message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	Ok(())
}

/// Add an ignored phrase.
///
/// Usage: `@Highlights ignore <phrase>`
pub async fn ignore(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let guild_id = require_guild!(ctx, message).0.try_into().unwrap();

	require_nonempty_args!(args, ctx, message);

	if args.len() < 3 {
		return error(
			ctx,
			message,
			"You can't ignore phrases shorter than 3 characters!",
		)
		.await;
	}

	let ignore = Ignore {
		user_id: message.author.id.0.try_into().unwrap(),
		guild_id,
		phrase: args.to_lowercase(),
	};

	if ignore.clone().exists().await? {
		return error(ctx, message, "You already ignored that phrase!").await;
	}

	ignore.insert().await?;

	success(ctx, message).await
}

/// Remove an ignored phrase.
///
/// Usage: `@Highlights unignore <phrase>`
pub async fn unignore(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let guild_id = require_guild!(ctx, message).0.try_into().unwrap();

	require_nonempty_args!(args, ctx, message);

	let ignore = Ignore {
		user_id: message.author.id.0.try_into().unwrap(),
		guild_id,
		phrase: args.to_lowercase(),
	};

	if !ignore.clone().exists().await? {
		return error(ctx, message, "You haven't ignored that phrase!").await;
	}

	ignore.delete().await?;

	success(ctx, message).await
}

/// List ignored phrases in the current guild, or in all guilds when used in DMs.
///
/// Usage: `@Highlights ignores`
pub async fn ignores(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("ignores");
	require_empty_args!(args, ctx, message);
	match message.guild_id {
		Some(guild_id) => {
			let ignores =
				Ignore::user_guild_ignores(message.author.id, guild_id)
					.await?
					.into_iter()
					.map(|ignore| ignore.phrase)
					.collect::<Vec<_>>();

			if ignores.is_empty() {
				return error(ctx, message, "You haven't ignored any phrases!")
					.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.context("Couldn't get guild to list ignores")?;

			let response = format!(
				"{}'s ignored phrases in {}:\n  - {}",
				message.author.name,
				guild_name,
				ignores.join("\n  - ")
			);

			message
				.channel_id
				.send_message(ctx, |m| {
					m.content(response).allowed_mentions(|m| m.empty_parse())
				})
				.await?;
		}
		None => {
			let ignores = Ignore::user_ignores(message.author.id).await?;

			if ignores.is_empty() {
				return error(ctx, message, "You haven't ignored any phrases!")
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

				let guild_id = GuildId(guild_id.try_into().unwrap());

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.await
					.context("Couldn't get guild to list ignores")?;

				write!(
					&mut response,
					"Your ignored phrases in {}:\n  – {}",
					guild_name,
					phrases.join("\n  – ")
				)
				.unwrap();
			}

			message.channel_id.say(ctx, response).await?;
		}
	}

	Ok(())
}

/// Remove keywords and ignores in a guild by ID.
///
/// Usage: `@Highlights remove-server <guild ID>`
pub async fn remove_server(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("removeserver");
	require_nonempty_args!(args, ctx, message);

	let guild_id = match args.parse() {
		Ok(id) => GuildId(id),
		Err(_) => return error(ctx, message, "Invalid server ID!").await,
	};

	let keywords_deleted =
		Keyword::delete_in_guild(message.author.id, guild_id).await?;

	let ignores_deleted =
		Ignore::delete_in_guild(message.author.id, guild_id).await?;

	if keywords_deleted + ignores_deleted == 0 {
		error(
			ctx,
			message,
			"You didn't have any keywords or ignores with that server ID!",
		)
		.await
	} else {
		success(ctx, message).await
	}
}

/// List keywords in the current guild, or in all guilds when used in DMs.
///
/// Usage: `@Highlights keywords`
pub async fn keywords(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("keywords");
	require_empty_args!(args, ctx, message);
	match message.guild_id {
		Some(guild_id) => {
			let guild_keywords =
				Keyword::user_guild_keywords(message.author.id, guild_id)
					.await?
					.into_iter()
					.map(|keyword| keyword.keyword)
					.collect::<Vec<_>>();

			let guild_channels =
				get_text_channels_in_guild(ctx, guild_id).await?;

			let mut channel_keywords = HashMap::new();

			for keyword in
				Keyword::user_channel_keywords(message.author.id).await?
			{
				let channel_id = match keyword.kind {
					KeywordKind::Channel(id) => {
						ChannelId(id.try_into().unwrap())
					}
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
				return error(
					ctx,
					message,
					"You haven't added any keywords yet!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
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

			message
				.channel_id
				.send_message(ctx, |m| {
					m.content(response).allowed_mentions(|m| m.empty_parse())
				})
				.await?;
		}
		None => {
			let keywords = Keyword::user_keywords(message.author.id).await?;

			if keywords.is_empty() {
				return error(
					ctx,
					message,
					"You haven't added any keywords yet!",
				)
				.await;
			}

			let mut keywords_by_guild = HashMap::new();

			let mut unknown_channel_keywords = HashMap::new();

			for keyword in keywords {
				match keyword.kind {
					KeywordKind::Guild(guild_id) => {
						let guild_id = GuildId(guild_id.try_into().unwrap());

						let guild_keywords = &mut keywords_by_guild
							.entry(guild_id)
							.or_insert_with(|| (Vec::new(), HashMap::new()))
							.0;

						guild_keywords.push(keyword.keyword);
					}
					KeywordKind::Channel(channel_id) => {
						let channel_id =
							ChannelId(channel_id.try_into().unwrap());

						let guild_id = ctx
							.cache
							.guild_channel_field(channel_id, |c| c.guild_id)
							.await;

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
					.await
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

			message.channel_id.say(ctx, response).await?;
		}
	}

	Ok(())
}
