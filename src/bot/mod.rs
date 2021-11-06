// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

mod responses;

mod commands;

#[macro_use]
mod util;
use util::{error, question};

mod highlighting;

use crate::{
	db::{Ignore, Keyword, Notification, UserState},
	global::{bot_mention, bot_nick_mention, init_mentions},
	log_discord_error,
	monitoring::Timer,
	regex,
	settings::settings,
};

use anyhow::{Context as _, Result};
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

use std::collections::HashMap;

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
						.await
						.context("Failed to handle command")?;
					highlighting::check_notify_user_state(&ctx, &message)
						.await
						.context("Failed to check user state")?;
					Ok(())
				}
				.await
			}
			None => {
				if message.guild_id.is_none() {
					async {
						handle_command(&ctx, &message, content.trim())
							.await
							.context("Failed to handle command")?;
						UserState::clear(message.author.id)
							.await
							.context("Failed to clear user state")?;
						Ok(())
					}
					.await
				} else {
					handle_keywords(&ctx, &message)
						.await
						.context("Failed to handle keywords")
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
		responses::delete_command_response(&ctx, channel_id, message_id).await;

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
) -> Result<()> {
	if message.guild_id.is_some() {
		let self_id = ctx.cache.current_user_id().await;

		let channel = ctx
			.cache
			.guild_channel(message.channel_id)
			.await
			.context("Nonexistent guild channel")?;

		let permissions = channel
			.permissions_for_user(ctx, self_id)
			.await
			.context("Failed to check permissions for self")?;

		if permissions.add_reactions() && !permissions.send_messages() {
			message
				.react(ctx, '🔇')
				.await
				.context("Failed to add muted reaction")?;

			return Ok(());
		} else if permissions.send_messages() && !permissions.add_reactions() {
			message
				.channel_id
				.say(
					ctx,
					"Sorry, I need permission to \
					add reactions to work right 😔",
				)
				.await
				.context(
					"Failed to send missing reaction permission message",
				)?;

			return Ok(());
		} else if !permissions.send_messages() && !permissions.add_reactions() {
			return Ok(());
		}
	}

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

	let result = {
		use commands::*;
		use tokio::task::spawn;

		let ctx = ctx.clone();
		let message = message.clone();
		let args = args.to_owned();

		match &*command {
			"add" => spawn(async move { add(&ctx, &message, &args).await }),
			"remove" => {
				spawn(async move { remove(&ctx, &message, &args).await })
			}
			"mute" => spawn(async move { mute(&ctx, &message, &args).await }),
			"unmute" => {
				spawn(async move { unmute(&ctx, &message, &args).await })
			}
			"ignore" => {
				spawn(async move { ignore(&ctx, &message, &args).await })
			}
			"unignore" => {
				spawn(async move { unignore(&ctx, &message, &args).await })
			}
			"block" => spawn(async move { block(&ctx, &message, &args).await }),
			"unblock" => {
				spawn(async move { unblock(&ctx, &message, &args).await })
			}
			"remove-server" => {
				spawn(async move { remove_server(&ctx, &message, &args).await })
			}
			"keywords" => {
				spawn(async move { keywords(&ctx, &message, &args).await })
			}
			"mutes" => spawn(async move { mutes(&ctx, &message, &args).await }),
			"ignores" => {
				spawn(async move { ignores(&ctx, &message, &args).await })
			}
			"blocks" => {
				spawn(async move { blocks(&ctx, &message, &args).await })
			}
			"opt-out" => {
				spawn(async move { opt_out(&ctx, &message, &args).await })
			}
			"opt-in" => {
				spawn(async move { opt_in(&ctx, &message, &args).await })
			}
			"help" => spawn(async move { help(&ctx, &message, &args).await }),
			"ping" => spawn(async move { ping(&ctx, &message, &args).await }),
			"about" => spawn(async move { about(&ctx, &message, &args).await }),
			_ => return question(&ctx, &message).await,
		}
		.await
		.map_err(anyhow::Error::from)
		.and_then(|r| r)
	};

	match result {
		Err(e) => {
			let _ = error(ctx, message, "Something went wrong running that :(")
				.await;
			Err(e.context(format!("Failed to run {} command", command)))
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
async fn handle_keywords(ctx: &Context, message: &Message) -> Result<()> {
	let _timer = Timer::notification("create");
	let guild_id = match message.guild_id {
		Some(id) => id,
		None => return Ok(()),
	};

	let channel_id = message.channel_id;

	let lowercase_content = message.content.to_lowercase();

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

		if highlighting::should_notify_keyword(
			ctx,
			message,
			&lowercase_content,
			&keyword,
			&ignores,
		)
		.await?
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

pub async fn init() {
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

	responses::init(&client).await;

	client.start().await.expect("Failed to run client");
}
