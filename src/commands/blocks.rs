// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use super::util::get_users_from_args;

use serenity::{client::Context, model::channel::Message};

use std::convert::TryInto;

use crate::{db::Block, error, monitoring::Timer, Error};

pub async fn block(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("block");

	require_nonempty_args!(args, ctx, message);

	let user_args = get_users_from_args(ctx, args).await;

	let not_found = user_args
		.not_found
		.iter()
		.map(|id| format!("<@{}>", id))
		.collect::<Vec<_>>();

	let mut blocked = vec![];
	let mut already_blocked = vec![];

	for user in user_args.found {
		let block = Block {
			user_id: message.author.id.0.try_into().unwrap(),
			blocked_id: user.id.0.try_into().unwrap(),
		};

		if block.clone().exists().await? {
			already_blocked.push(format!("<@{}> ({})", user.id, user.name));
		} else {
			blocked.push(format!("<@{}> ({})", user.id, user.name));
			block.insert().await?;
		}
	}

	let mut msg = String::with_capacity(45);

	if !blocked.is_empty() {
		msg.push_str("Blocked users: ");
		msg.push_str(&blocked.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !already_blocked.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Users already blocked: ");
		msg.push_str(&already_blocked.join(", "));

		message.react(ctx, '❌').await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find users: ");
		msg.push_str(&not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	if !user_args.invalid.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Invalid arguments (use mentions or IDs): ");
		msg.push_str(&user_args.invalid.join(", "));

		if already_blocked.is_empty() {
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

pub async fn unblock(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("unblock");

	require_nonempty_args!(args, ctx, message);

	let user_args = get_users_from_args(ctx, args).await;

	let mut not_found = user_args
		.not_found
		.iter()
		.map(|id| format!("<@{}>", id))
		.collect::<Vec<_>>();

	let mut unblocked = vec![];
	let mut not_blocked = vec![];

	for user in user_args.found {
		let block = Block {
			user_id: message.author.id.0.try_into().unwrap(),
			blocked_id: user.id.0.try_into().unwrap(),
		};

		if !block.clone().exists().await? {
			not_blocked.push(format!("<@{}> ({})", user.id, user.name));
		} else {
			unblocked.push(format!("<@{}> ({})", user.id, user.name));
			block.delete().await?;
		}
	}

	for id in user_args.not_found {
		let block = Block {
			user_id: message.author.id.0.try_into().unwrap(),
			blocked_id: id.try_into().unwrap(),
		};

		if !block.clone().exists().await? {
			not_found.push(format!("<@{0}> ({0})", id));
		} else {
			unblocked.push(format!("<@{0}> ({0})", id));
			block.delete().await?;
		}
	}

	let mut msg = String::with_capacity(45);

	if !unblocked.is_empty() {
		msg.push_str("Unblocked users: ");
		msg.push_str(&unblocked.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !not_blocked.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Users weren't blocked: ");
		msg.push_str(&not_blocked.join(", "));

		message.react(ctx, '❌').await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find users: ");
		msg.push_str(&not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	if !user_args.invalid.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Invalid arguments (use mentions or IDs): ");
		msg.push_str(&user_args.invalid.join(", "));

		if not_blocked.is_empty() {
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

pub async fn blocks(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("blocks");
	require_empty_args!(args, ctx, message);

	let blocks = Block::user_blocks(message.author.id)
		.await?
		.into_iter()
		.map(|block| format!("<@{}>", block.blocked_id))
		.collect::<Vec<_>>();

	if blocks.is_empty() {
		error(ctx, message, "You haven't blocked any users!").await?;
	} else {
		let msg = format!(
			"{}'s blocked users:\n  - {}",
			message.author.name,
			blocks.join("\n  - ")
		);

		message
			.channel_id
			.send_message(ctx, |m| {
				m.content(msg).allowed_mentions(|m| m.empty_parse())
			})
			.await?;
	}
	Ok(())
}
