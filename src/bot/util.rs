// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros.

use anyhow::{Context as _, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serenity::{
	client::Context,
	http::{error::ErrorResponse, CacheHttp},
	model::{
		channel::{GuildChannel, Message},
		guild::{Guild, PartialGuild},
		id::UserId,
		interactions::{
			application_command::ApplicationCommandInteraction as Command,
			InteractionApplicationCommandCallbackDataFlags as ResponseFlags,
		},
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

/// Responds to a command with a ✅ emoji.
#[inline]
pub async fn success(ctx: &Context, command: &Command) -> Result<()> {
	respond_eph(ctx, command, "✅\u{200b}") // zero-width space to force small emoji
		.await
		.context("Failed to add success reaction")?;

	Ok(())
}

/// Responds to a command with the given message.
pub async fn respond<S: Display>(
	ctx: &Context,
	command: &Command,
	response: S,
) -> Result<()> {
	command
		.create_interaction_response(ctx, |r| {
			r.interaction_response_data(|m| m.content(response))
		})
		.await
		.context("Failed to send command response")?;

	Ok(())
}

/// Responds to a command with the given message ephemerally.
pub async fn respond_eph<S: Display>(
	ctx: &Context,
	command: &Command,
	response: S,
) -> Result<()> {
	command
		.create_interaction_response(ctx, |r| {
			r.interaction_response_data(|m| {
				m.flags(ResponseFlags::EPHEMERAL).content(response)
			})
		})
		.await
		.context("Failed to send command response")?;

	Ok(())
}

pub async fn followup_eph<S: Display>(
	ctx: &Context,
	command: &Command,
	response: S,
) -> Result<()> {
	command
		.create_followup_message(ctx, |r| {
			r.flags(ResponseFlags::EPHEMERAL).content(response)
		})
		.await
		.context("Failed to send command followup")?;

	Ok(())
}

/// Determines if a user with the given ID can read messages in the provided `GuildChannel`.
pub async fn user_can_read_channel(
	ctx: &impl CacheHttp,
	channel: &GuildChannel,
	user_id: UserId,
) -> Result<Option<bool>> {
	#[allow(clippy::large_enum_variant)]
	enum MaybePartialGuild {
		Partial(PartialGuild),
		FullGuild(Guild),
	}

	use MaybePartialGuild::*;

	let guild = match ctx.cache().unwrap().guild(channel.guild_id).await {
		Some(g) => FullGuild(g),
		None => Partial(ctx.http().get_guild(channel.guild_id.0).await?),
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
		FullGuild(g) => g.user_permissions_in(channel, &member)?,
		Partial(g) => g.user_permissions_in(channel, &member)?,
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
