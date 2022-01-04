// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for opting out (and in) of having messages highlighted.

use anyhow::Result;

use serenity::{
	client::Context,
	model::interactions::application_command::ApplicationCommandInteraction as Command,
};

use crate::{
	bot::util::{respond_eph, success},
	db::OptOut,
	monitoring::Timer,
};

/// Opt-out of being highlighted.
///
/// Usage:
/// - `/opt-out`
pub async fn opt_out(ctx: &Context, command: Command) -> Result<()> {
	let _timer = Timer::command("opt-out");

	let opt_out = OptOut {
		user_id: command.user.id,
	};

	if opt_out.clone().exists().await? {
		return respond_eph(ctx, &command, "❌ You already opted out!").await;
	}

	opt_out.insert().await?;

	success(ctx, &command).await
}

/// Opt-in to being highlighted, after having opted out.
///
/// Usage:
/// - `/opt-in`
pub async fn opt_in(ctx: &Context, command: Command) -> Result<()> {
	let _timer = Timer::command("opt-in");

	let opt_out = OptOut {
		user_id: command.user.id,
	};

	if !opt_out.clone().exists().await? {
		return respond_eph(ctx, &command, "❌ You haven't opted out!").await;
	}

	opt_out.delete().await?;

	success(ctx, &command).await
}
