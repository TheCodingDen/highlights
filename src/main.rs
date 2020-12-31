// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Highlights is a simple but flexible keyword highlighting bot for Discord.
//!
//! The code for highlights is organized into mostly independent modules. This module handles
//! creating the client and registering event listeners.

#![type_length_limit = "20000000"]

mod commands;

pub mod db;
use db::{Ignore, Keyword, Notification, UserState};

mod error;
pub use error::Error;

pub mod settings;

pub mod global;
use global::{
	bot_mention, bot_nick_mention, init_mentions, init_settings, settings,
};

mod highlighting;

pub mod monitoring;

pub mod reporting;

#[macro_use]
pub mod util;
use util::{error, question};

use serenity::{
	client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler},
	model::{
		channel::Message,
		event::MessageUpdateEvent,
		gateway::{Activity, Ready},
		id::{ChannelId, GuildId, MessageId, UserId},
	},
};
use tokio::task;

use monitoring::Timer;
use std::{collections::HashMap, convert::TryInto};

/// Type to serve as an event handler.
struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
	/// Message listener to execute commands or check for notifications.
	///
	/// This function essentially just checks the message to see if it's a command; if it is, then
	/// [`handle_command`](handle_command) is called. If not, [`handle_keywords`](handle_keywords)
	/// is called to check if there are any keywords to notify others of.
	async fn message(&self, ctx: Context, message: Message) {
		if message.author.bot {
			return;
		}

		let content = message.content.as_str();

		let result = match content
			.strip_prefix(bot_mention())
			.or_else(|| content.strip_prefix(bot_nick_mention()))
		{
			Some(command_content) => {
				async {
					handle_command(&ctx, &message, command_content.trim())
						.await?;
					highlighting::check_notify_user_state(&ctx, &message)
						.await?;
					Ok(())
				}
				.await
			}
			None => {
				if message.guild_id.is_none() {
					async {
						handle_command(&ctx, &message, content.trim()).await?;
						UserState::clear(message.author.id).await?;
						Ok(())
					}
					.await
				} else {
					handle_keywords(&ctx, &message).await
				}
			}
		};

		if let Err(e) = &result {
			log_discord_error!(in message.channel_id, by message.author.id, e);
		}
	}

	/// Deletes sent notifications if their original messages were deleted.
	async fn message_delete(
		&self,
		ctx: Context,
		channel_id: ChannelId,
		message_id: MessageId,
		guild_id: Option<GuildId>,
	) {
		if guild_id.is_none() {
			return;
		}

		let notifications =
			match Notification::notifications_of_message(message_id).await {
				Ok(n) => n,
				Err(e) => {
					log_discord_error!(in channel_id, deleted message_id, e);
					return;
				}
			};

		if notifications.is_empty() {
			return;
		}

		let _timer = Timer::notification("delete");

		highlighting::delete_sent_notifications(
			&ctx,
			channel_id,
			&notifications,
		)
		.await;

		if let Err(e) =
			Notification::delete_notifications_of_message(message_id).await
		{
			log_discord_error!(in channel_id, deleted message_id, e);
		}
	}

	/// Edits notifications if their original messages are edited.
	///
	/// Edits the content of a notification to reflect the new content of the original message if
	/// the original message still contains the keyword the notification was created for. Deletes
	/// the notification if the new content no longer contains the keyword.
	async fn message_update(
		&self,
		ctx: Context,
		_: Option<Message>,
		new: Option<Message>,
		event: MessageUpdateEvent,
	) {
		let guild_id = match event.guild_id {
			Some(g) => g,
			None => return,
		};

		let notifications =
			match Notification::notifications_of_message(event.id).await {
				Ok(n) => n,
				Err(e) => {
					log_discord_error!(in event.channel_id, edited event.id, e);
					return;
				}
			};

		if notifications.is_empty() {
			return;
		}

		let _timer = Timer::notification("edit");

		let message = match new {
			Some(m) => m,
			None => {
				match ctx.http.get_message(event.channel_id.0, event.id.0).await
				{
					Ok(m) => m,
					Err(e) => {
						log_discord_error!(in event.channel_id, edited event.id, e);
						return;
					}
				}
			}
		};

		highlighting::update_sent_notifications(
			&ctx,
			event.channel_id,
			guild_id,
			message,
			notifications,
		)
		.await;
	}

	/// Runs minor setup for when the bot starts.
	///
	/// This calls [`init_mentions`](crate::global::init_mentions), sets the bot's status, and
	/// logs a ready message.
	async fn ready(&self, ctx: Context, ready: Ready) {
		init_mentions(ready.user.id);

		let username = ctx.cache.current_user_field(|u| u.name.clone()).await;

		ctx.set_activity(Activity::listening(&format!("@{} help", username)))
			.await;

		log::info!("Ready to highlight!");
	}
}

/// Handles a command.
///
/// This function splits message content to determine if there is a command to be handled, then
/// dispatches to the appropriate function from [`commands`](commands).
async fn handle_command(
	ctx: &Context,
	message: &Message,
	content: &str,
) -> Result<(), Error> {
	let (command, args) = {
		let mut iter = regex!(r" +").splitn(content, 2);

		let command = iter.next().map(str::to_lowercase);

		let args = iter.next().map(|s| s.trim()).unwrap_or("");

		(command, args)
	};

	let command = match command {
		Some(command) => command,
		None => return question(ctx, message).await,
	};

	let result = match &*command {
		"add" => commands::add(ctx, message, args).await,
		"remove" => commands::remove(ctx, message, args).await,
		"mute" => commands::mute(ctx, message, args).await,
		"unmute" => commands::unmute(ctx, message, args).await,
		"ignore" => commands::ignore(ctx, message, args).await,
		"unignore" => commands::unignore(ctx, message, args).await,
		"block" => commands::block(ctx, message, args).await,
		"unblock" => commands::unblock(ctx, message, args).await,
		"remove-server" => commands::remove_server(ctx, message, args).await,
		"keywords" => commands::keywords(ctx, message, args).await,
		"mutes" => commands::mutes(ctx, message, args).await,
		"ignores" => commands::ignores(ctx, message, args).await,
		"blocks" => commands::blocks(ctx, message, args).await,
		"help" => commands::help(ctx, message, args).await,
		"ping" => commands::ping(ctx, message, args).await,
		"about" => commands::about(ctx, message, args).await,
		_ => return question(ctx, message).await,
	};

	match result {
		Err(e) => {
			let _ = error(ctx, message, "Something went wrong running that :(")
				.await;
			Err(e)
		}
		Ok(_) => Ok(()),
	}
}

/// Handles any keywords present in a message.
///
/// This function queries for any keywords that could be relevant to the sent message with
/// [`get_relevant_keywords`](Keyword::get_relevant_keywords), collects [`Ignore`](Ignore)s for any
/// users with those keywords. It uses (`should_notify_keyword`)[highlighting::should_notify_keyword]
/// to determine if there is a keyword that should be highlighted, then calls
/// (`notify_keyword`)[highlighting::notify_keyword].
async fn handle_keywords(
	ctx: &Context,
	message: &Message,
) -> Result<(), Error> {
	let _timer = Timer::notification("create");
	let guild_id = match message.guild_id {
		Some(id) => id,
		None => return Ok(()),
	};

	let channel_id = message.channel_id;

	let keywords =
		Keyword::get_relevant_keywords(guild_id, channel_id, message.author.id)
			.await?;

	let mut ignores_by_user = HashMap::new();

	for keyword in keywords {
		let ignores = match ignores_by_user.get(&keyword.user_id) {
			Some(ignores) => ignores,
			None => {
				let user_ignores = Ignore::user_guild_ignores(
					UserId(keyword.user_id.try_into().unwrap()),
					guild_id,
				)
				.await?;
				ignores_by_user
					.entry(keyword.user_id)
					.or_insert(user_ignores)
			}
		};

		if highlighting::should_notify_keyword(ctx, message, &keyword, &ignores)
			.await?
			.is_some()
		{
			let ctx = ctx.clone();
			task::spawn(highlighting::notify_keyword(
				ctx,
				message.clone(),
				keyword,
				ignores.clone(),
				guild_id,
			));
		}
	}

	Ok(())
}

/// Entrypoint function to initialize other modules and start the Discord client.
#[tokio::main]
async fn main() {
	init_settings();

	reporting::init();

	db::init();

	monitoring::init();

	let mut client = Client::builder(&settings().bot.token)
		.event_handler(Handler)
		.intents(
			GatewayIntents::DIRECT_MESSAGES
				| GatewayIntents::GUILD_MESSAGE_REACTIONS
				| GatewayIntents::GUILD_MESSAGES
				| GatewayIntents::GUILDS
				| GatewayIntents::GUILD_MEMBERS,
		)
		.await
		.expect("Failed to create client");

	client.start().await.expect("Failed to run client");
}
