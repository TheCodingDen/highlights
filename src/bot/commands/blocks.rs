// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for adding, removing, and listing blocked users.

use anyhow::{Context as _, Result};
use serenity::{
	client::Context,
	model::application::interaction::application_command::ApplicationCommandInteraction as Command,
};

use crate::{bot::util::respond_eph, db::Block};

/// Block a user.
///
/// Usage: `/block <user>`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn block(ctx: Context, mut command: Command) -> Result<()> {
	check_opt_out!(ctx, command);

	let user = command
		.data
		.resolved
		.users
		.drain()
		.next()
		.map(|(_, user)| user)
		.context("User to block not provided")?;

	if user.id == command.user.id {
		return respond_eph(&ctx, &command, "❌ You can't block yourself!")
			.await;
	}

	let block = Block {
		user_id: command.user.id,
		blocked_id: user.id,
	};

	if block.clone().exists().await? {
		respond_eph(
			&ctx,
			&command,
			format!("❌ You already blocked <@{}>!", user.id),
		)
		.await
	} else {
		block.insert().await?;
		respond_eph(&ctx, &command, format!("✅ Blocked <@{}>", user.id)).await
	}
}

/// Unblock a user.
///
/// Usage: `/unblock <user>`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn unblock(ctx: Context, mut command: Command) -> Result<()> {
	check_opt_out!(ctx, command);

	let user = command
		.data
		.resolved
		.users
		.drain()
		.next()
		.map(|(_, user)| user)
		.context("User to unblock not provided")?;

	if user.id == command.user.id {
		return respond_eph(&ctx, &command, "❌ You can't unblock yourself!")
			.await;
	}

	let block = Block {
		user_id: command.user.id,
		blocked_id: user.id,
	};

	if !block.clone().exists().await? {
		respond_eph(
			&ctx,
			&command,
			format!("❌ You haven't blocked <@{}>!", user.id),
		)
		.await
	} else {
		block.delete().await?;
		respond_eph(&ctx, &command, format!("✅ Unblocked <@{}>", user.id))
			.await
	}
}

/// Lists blocked users.
///
/// Usage: `/blocks`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn blocks(ctx: Context, command: Command) -> Result<()> {
	check_opt_out!(ctx, command);

	let blocks = Block::user_blocks(command.user.id)
		.await?
		.into_iter()
		.map(|block| format!("<@{}>", block.blocked_id))
		.collect::<Vec<_>>();

	if blocks.is_empty() {
		respond_eph(&ctx, &command, "You haven't blocked any users!").await
	} else {
		let msg = format!(
			"{}'s blocked users:\n  - {}",
			command.user.name,
			blocks.join("\n  - ")
		);

		respond_eph(&ctx, &command, msg).await
	}
}
