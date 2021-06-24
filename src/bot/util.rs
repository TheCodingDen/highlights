// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros.

use anyhow::{Context as _, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serenity::{
	client::Context,
	http::error::ErrorResponse,
	model::{
		channel::{GuildChannel, Message},
		guild::{Guild, PartialGuild},
		id::UserId,
	},
	prelude::HttpError,
	Error as SerenityError,
};
use std::fmt::Display;

use super::responses::insert_command_response;

/// Logs an error that happened handling a command or keyword in Discord.
///
/// # Usage
/// ```
/// #let channel_id = 4;
/// #let author_id = 5;
/// #let some_result = Err::<(), &'static str>("Uh oh!");
/// if let Err(e) = some_result {
/// 	log_discord_error!(in channel_id, by author_id, e);
/// }
/// ```
#[macro_export]
macro_rules! log_discord_error {
	(in $channel_id:expr, by $user_id:expr, $error:expr) => {
		log::error!(
			"Error in <#{0}> ({0}) by <@{1}> ({1}):\n{2:?}",
			$channel_id,
			$user_id,
			$error
		);
	};
	(in $channel_id:expr, deleted $message_id:expr, $error:expr) => {
		log::error!(
			"Error in <#{0}> ({0}), handling deleted message {1}:\n{2:?}",
			$channel_id,
			$message_id,
			$error
		);
	};
	(in $channel_id:expr, edited $message_id:expr, $error:expr) => {
		log::error!(
			"Error in <#{0}> ({0}), handling edited message {1}:\n{2:?}",
			$channel_id,
			$message_id,
			$error
		);
	};
}

/// Creates a [`Regex`](::regex::Regex) from a regex literal.
///
/// The created regex is static and will only be constructed once through the life of the program.
///
/// # Example
/// ```
/// let re = regex!(r"[a-z]+");
///
/// assert!(re.is_match("hello"));
/// ```
#[macro_export]
macro_rules! regex {
	($re:literal $(,)?) => {{
		static RE: once_cell::sync::OnceCell<regex::Regex> =
			once_cell::sync::OnceCell::new();
		RE.get_or_init(|| regex::Regex::new($re).unwrap())
	}};
}

/// Regex for symbols used in Discord-flavor markdown.
///
/// Equivalent to the regex `[_*()\[\]~`]`. It includes `()` and `[]` because these are treated as
/// part of links in embeds, where this is frequently used.
pub static MD_SYMBOL_REGEX: Lazy<Regex, fn() -> Regex> =
	Lazy::new(|| Regex::new(r"[_*()\[\]~`]").unwrap());

/// Reacts to a message with a ✅ emoji.
pub async fn success(ctx: &Context, message: &Message) -> Result<()> {
	message
		.react(ctx, '✅')
		.await
		.context("Failed to add success reaction")?;

	Ok(())
}

/// Reacts to a message with a ❓ emoji.
pub async fn question(ctx: &Context, message: &Message) -> Result<()> {
	message
		.react(ctx, '❓')
		.await
		.context("Failed to add question reaction")?;

	Ok(())
}

/// Reacts to a message with a ❌ emoji and send the given response in the same channel.
///
/// The response message is stripped of mentions when sent to Discord.
pub async fn error<S: Display>(
	ctx: &Context,
	message: &Message,
	response: S,
) -> Result<()> {
	let _ = message.react(ctx, '❌').await;

	let response = message
		.channel_id
		.send_message(ctx, |m| {
			m.content(response).allowed_mentions(|m| m.empty_parse())
		})
		.await
		.context("Failed to send command usage error message")?;

	insert_command_response(ctx, message.id, response.id).await;

	Ok(())
}

/// Determines if a user with the given ID can read messages in the provided `GuildChannel`.
pub async fn user_can_read_channel(
	ctx: &Context,
	channel: &GuildChannel,
	user_id: UserId,
) -> Result<Option<bool>> {
	#[allow(clippy::large_enum_variant)]
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
		FullGuild(g) => optional_result(g.member(ctx, user_id).await)?,
		Partial(g) => optional_result(g.member(ctx, user_id).await)?,
	};

	let member = match member {
		Some(m) => m,
		None => return Ok(None),
	};

	let permissions = match &guild {
		FullGuild(g) => g.user_permissions_in(&channel, &member)?,
		Partial(g) => g.user_permissions_in(&channel, &member)?,
	};

	Ok(Some(permissions.read_messages()))
}

/// Makes the result of an HTTP call optional.
///
/// If the given `Result` is an `Err` containing an error with a 404 HTTP error, `Ok(None)` is
/// returned. Otherwise, the `Result` is returned, `Ok(x)` being replaced with `Ok(Some(x))`.
pub fn optional_result<T>(
	res: Result<T, SerenityError>,
) -> Result<Option<T>, SerenityError> {
	match res {
		Ok(m) => Ok(Some(m)),
		Err(SerenityError::Http(err)) => match &*err {
			HttpError::UnsuccessfulRequest(ErrorResponse {
				status_code,
				..
			}) if status_code.as_u16() == 404 => Ok(None),
			_ => Err(SerenityError::Http(err)),
		},
		Err(err) => Err(err),
	}
}
