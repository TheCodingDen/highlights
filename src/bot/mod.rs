// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

mod responses;

mod commands;

#[macro_use]
mod util;
use futures_util::{stream, StreamExt, TryStreamExt};
use tinyvec::TinyVec;
use util::{error, question};

mod highlighting;

use crate::{
	db::{Ignore, Keyword, Notification, UserState},
	global::{bot_mention, bot_nick_mention, init_cache, init_mentions},
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
		id::{ChannelId, GuildId, MessageId},
		interactions::Interaction,
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

		if let Err(e) = handle_keywords(&ctx, &message)
			.await
			.context("Failed to handle keywords")
		{
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
				Ok(n) => n
					.into_iter()
					.map(|notification| {
						(
							notification.user_id,
							notification.notification_message,
						)
					})
					.collect::<Vec<_>>(),
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
			message_id,
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

		ctx.set_activity(Activity::listening("/help")).await;

		commands::create_commands(ctx).await;

		log::info!("Ready to highlight!");
	}

	/// Responds to slash commands.
	async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
		let command = match interaction {
			Interaction::ApplicationCommand(cmd) => cmd,
			_ => return,
		};
		let name = command.data.name.clone();
		let channel_id = command.channel_id;
		let user_id = command.user.id;

		let result = {
			use commands::*;
			use tokio::task::spawn;

			let ctx = ctx.clone();

			match &*name {
				// "add" => spawn(async move { add(&ctx, &message, &args).await }),
				// "remove" => {
				// 	spawn(async move { remove(&ctx, &message, &args).await })
				// }
				// "mute" => {
				// 	spawn(async move { mute(&ctx, &message, &args).await })
				// }
				// "unmute" => {
				// 	spawn(async move { unmute(&ctx, &message, &args).await })
				// }
				// "ignore" => {
				// 	spawn(async move { ignore(&ctx, &message, &args).await })
				// }
				// "unignore" => {
				// 	spawn(async move { unignore(&ctx, &message, &args).await })
				// }
				// "block" => {
				// 	spawn(async move { block(&ctx, &message, &args).await })
				// }
				// "unblock" => {
				// 	spawn(async move { unblock(&ctx, &message, &args).await })
				// }
				// "remove-server" => {
				// 	spawn(
				// 		async move { remove_server(&ctx, &message, &args).await },
				// 	)
				// }
				// "keywords" => {
				// 	spawn(async move { keywords(&ctx, &message, &args).await })
				// }
				// "mutes" => {
				// 	spawn(async move { mutes(&ctx, &message, &args).await })
				// }
				// "ignores" => {
				// 	spawn(async move { ignores(&ctx, &message, &args).await })
				// }
				// "blocks" => {
				// 	spawn(async move { blocks(&ctx, &message, &args).await })
				// }
				// "opt-out" => {
				// 	spawn(async move { opt_out(&ctx, &message, &args).await })
				// }
				// "opt-in" => {
				// 	spawn(async move { opt_in(&ctx, &message, &args).await })
				// }
				"help" => spawn(async move { help(&ctx, command).await }),
				"ping" => spawn(async move { ping(&ctx, command).await }),
				"about" => spawn(async move { about(&ctx, command).await }),
				_ => {
					let err = anyhow::anyhow!(
						"Unknown slash command received: {}",
						name
					);

					spawn(async move { Err(err) })
				}
			}
			.await
			.map_err(anyhow::Error::from)
			.and_then(|r| r)
		};

		if let Err(e) = result {
			// TODO: tell the user it failed

			log::error!(
				"Error in <#{0}> ({0}) by <@{1}> ({1}):\n{2:?}",
				channel_id,
				user_id,
				e
			);
		}
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

	let lowercase_content = &message.content.to_lowercase();

	let keywords_by_user =
		Keyword::get_relevant_keywords(guild_id, channel_id, message.author.id)
			.await?
			.into_iter()
			.fold(HashMap::new(), |mut map, keyword| {
				map.entry(keyword.user_id)
					.or_insert_with(|| tinyvec::tiny_vec![[Keyword; 2]])
					.push(keyword);
				map
			});

	let mut ignores_by_user = HashMap::new();

	for (user_id, keywords) in keywords_by_user {
		let ignores = match ignores_by_user.get(&user_id) {
			Some(ignores) => ignores,
			None => {
				let user_ignores =
					Ignore::user_guild_ignores(user_id, guild_id).await?;
				ignores_by_user.entry(user_id).or_insert(user_ignores)
			}
		};

		let keywords = stream::iter(keywords)
			.map(Ok::<_, anyhow::Error>) // convert to a TryStream
			.try_filter_map(|keyword| async move {
				Ok(highlighting::should_notify_keyword(
					ctx,
					message,
					lowercase_content,
					&keyword,
					ignores,
				)
				.await?
				.then(|| keyword))
			})
			.try_collect::<TinyVec<[Keyword; 2]>>()
			.await?;

		let ctx = ctx.clone();
		task::spawn(highlighting::notify_keywords(
			ctx,
			message.clone(),
			keywords,
			ignores.clone(),
			user_id,
			guild_id,
		));
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
		.application_id(settings().bot.application_id)
		.await
		.expect("Failed to create client");

	responses::init(&client).await;

	init_cache(client.cache_and_http.clone());

	client.start().await.expect("Failed to run client");
}
