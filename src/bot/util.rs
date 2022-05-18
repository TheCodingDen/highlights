// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Miscellaneous utility functions and macros.

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	http::{error::ErrorResponse, CacheHttp},
	model::{
		channel::GuildChannel,
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

/// Responds to a command with a ✅ emoji.
#[inline]
pub(crate) async fn success(ctx: &Context, command: &Command) -> Result<()> {
	respond_eph(ctx, command, "✅\u{200b}") // zero-width space to force small emoji
		.await
		.context("Failed to add success reaction")?;

	Ok(())
}

/// Responds to a command with the given message.
pub(crate) async fn respond<S: Display>(
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
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn respond_eph<S: Display>(
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

#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn followup_eph<S: Display>(
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
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %user_id,
		channel_id = %channel.id,
	)
)]
pub(crate) async fn user_can_read_channel(
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
pub(crate) fn optional_result<T>(
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
