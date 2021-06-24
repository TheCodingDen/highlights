// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for opting out (and in) of having messages highlighted.

use anyhow::Result;

use serenity::{client::Context, model::channel::Message};

use crate::{
	bot::util::{error, success},
	db::OptOut,
	monitoring::Timer,
};

/// Opt-out of being highlighted.
///
/// Usage:
/// - `@Highlights opt-out`
pub async fn opt_out(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("opt-out");

	require_empty_args!(args, ctx, message);

	let opt_out = OptOut {
		user_id: message.author.id,
	};

	if opt_out.clone().exists().await? {
		return error(ctx, message, "You already opted out!").await;
	}

	opt_out.insert().await?;

	success(ctx, message).await
}

/// Opt-in to being highlighted, after having opted out.
///
/// Usage:
/// - `@Highlights opt-in`
pub async fn opt_in(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<()> {
	let _timer = Timer::command("opt-in");

	require_empty_args!(args, ctx, message);

	let opt_out = OptOut {
		user_id: message.author.id,
	};

	if !opt_out.clone().exists().await? {
		return error(ctx, message, "You haven't opted out!").await;
	}

	opt_out.delete().await?;

	success(ctx, message).await
}
