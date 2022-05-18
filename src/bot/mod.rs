// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Discord client creation and behavior.

#[macro_use]
mod util;
mod commands;
mod highlighting;

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context as _, Result};
use futures_util::{
	stream::{self, FuturesUnordered},
	StreamExt, TryStreamExt,
};
use serenity::{
	builder::CreateEmbed,
	client::{bridge::gateway::ShardManager, Client, Context, EventHandler},
	http::{
		error::{DiscordJsonError, ErrorResponse},
		HttpError,
	},
	model::{
		channel::Message,
		event::MessageUpdateEvent,
		gateway::{Activity, GatewayIntents, Ready},
		id::{ChannelId, GuildId, MessageId},
		interactions::{
			application_command::ApplicationCommandInteraction as Command,
			Interaction,
			InteractionApplicationCommandCallbackDataFlags as ResponseFlags,
		},
	},
	prelude::{Mutex, TypeMapKey},
	Error as SerenityError,
};
use tinyvec::TinyVec;
use tracing::{
	field::{display, Empty},
	info_span, Span,
};

use self::highlighting::CachedMessages;
use crate::{
	db::{Ignore, Keyword, Notification},
	global::ERROR_COLOR,
	settings::settings,
};

/// Type to serve as an event handler.
struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
	/// Message listener to check for keywords.
	///
	/// Calls [`handle_keywords`] for any non-bot messages in a guild to check
	/// if there are any keywords to notify others of.
	async fn message(&self, ctx: Context, message: Message) {
		if message.author.bot {
			return;
		}

		let guild_id = match message.guild_id {
			Some(id) => id,
			None => return,
		};

		handle_keywords(&ctx, &message, guild_id).await;
	}

	/// Message listener to check messages for notifications to delete.
	///
	/// Calls [`handle_deletion`] for any non-bot messages in a guild to check
	/// if there are any notifications of that message to delete.
	async fn message_delete(
		&self,
		ctx: Context,
		channel_id: ChannelId,
		message_id: MessageId,
		guild_id: Option<GuildId>,
	) {
		let guild_id = match guild_id {
			Some(id) => id,
			None => return,
		};

		handle_deletion(ctx, channel_id, message_id, guild_id).await;
	}

	/// Message listener to edit notifications
	///
	/// Calls [`handle_update`] for any non-bot messages in a guild to check if
	/// there are any notifications of that message to update.
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

		handle_update(ctx, new, event, guild_id).await;
	}

	/// Runs minor setup for when the bot starts.
	///
	/// Calls [`ready`].
	async fn ready(&self, ctx: Context, _: Ready) {
		ready(ctx).await;
	}

	/// Responds to slash commands.
	async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
		let command = match interaction {
			Interaction::ApplicationCommand(cmd) => cmd,
			_ => return,
		};

		handle_command(ctx, command).await;
	}
}

/// Sets the bot's activity to "Listening to /help" and
/// [creates slash commands](commands::create_commands).
async fn ready(ctx: Context) {
	let span = info_span!(parent: None, "ready");

	let _entered = span.enter();

	ctx.set_activity(Activity::listening("/help")).await;

	commands::create_commands(ctx).await;

	tracing::info!("Ready to highlight!");
}

/// Finds notifications for an updated message and uses
/// [`update_sent_notifications`](highlighting::update_sent_notifications) to
/// update them.
async fn handle_update(
	ctx: Context,
	new: Option<Message>,
	event: MessageUpdateEvent,
	guild_id: GuildId,
) {
	let span = info_span!(
		parent: None,
		"message_update",
		message_id = %event.id,
		author_id = Empty,
		channel_id = %event.channel_id,
		guild_id = %guild_id,
	);

	let _entered = span.enter();

	let content = match event.content.as_ref() {
		Some(s) => s.clone(),
		None => return,
	};

	if let Some(old_content) = ctx
		.data
		.write()
		.await
		.get_mut::<CachedMessages>()
		.expect("No message cache")
		.get_mut(&event.id)
	{
		*old_content = content;
	}

	let notifications = match Notification::notifications_of_message(event.id)
		.await
		.context("Failed to get notifications for message")
	{
		Ok(n) => n,
		Err(e) => {
			tracing::error!("{:?}", e);
			return;
		}
	};

	if notifications.is_empty() {
		return;
	}

	let message = match new {
		Some(m) => m,
		None => {
			match ctx
				.http
				.get_message(event.channel_id.0, event.id.0)
				.await
				.context("Failed to fetch updated message")
			{
				Ok(m) => m,
				Err(e) => {
					tracing::error!("{:?}", e);
					return;
				}
			}
		}
	};

	span.record("author_id", &display(message.author.id));

	highlighting::update_sent_notifications(
		&ctx,
		guild_id,
		message,
		notifications,
	)
	.await;
}

/// Finds notifications for a deleted message and uses
/// [`delete_sent_notifications`](highlighting::delete_sent_notifications) to
/// delete them.
async fn handle_deletion(
	ctx: Context,
	channel_id: ChannelId,
	message_id: MessageId,
	guild_id: GuildId,
) {
	let span = info_span!(
		parent: None,
		"handle_deletion",
		channel_id = %channel_id,
		message_id = %message_id,
		guild_id = %guild_id,
	);

	let _entered = span.enter();

	ctx.data
		.write()
		.await
		.get_mut::<CachedMessages>()
		.expect("No message cache")
		.remove(&message_id);

	let notifications =
		match Notification::notifications_of_message(message_id).await {
			Ok(n) => n
				.into_iter()
				.map(|notification| {
					(notification.user_id, notification.notification_message)
				})
				.collect::<Vec<_>>(),
			Err(e) => {
				tracing::error!("{:?}", e);
				return;
			}
		};

	if notifications.is_empty() {
		return;
	}

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
		tracing::error!("{:?}", e);
	}
}

/// Handles any keywords present in a message.
///
/// This function queries for any keywords that could be relevant to the sent
/// message with [`get_relevant_keywords`](Keyword::get_relevant_keywords),
/// collects [`Ignore`]s for any users with those keywords. It then
/// calls [`notify_keywords`](highlighting::notify_keywords).
async fn handle_keywords(ctx: &Context, message: &Message, guild_id: GuildId) {
	let res: Result<()> = async move {
		let channel_id = message.channel_id;

		let span = info_span!(
			parent: None,
			"handle_keywords",
			message_id = %message.id,
			channel_id = %channel_id,
			author_id = %message.author.id,
			guild_id = %guild_id,
		);

		let _entered = span.enter();

		let lowercase_content = &message.content.to_lowercase();

		let keywords_by_user = Keyword::get_relevant_keywords(
			guild_id,
			channel_id,
			message.author.id,
		)
		.await?
		.into_iter()
		.fold(HashMap::new(), |mut map, keyword| {
			map.entry(keyword.user_id)
				.or_insert_with(|| tinyvec::tiny_vec![[Keyword; 2]])
				.push(keyword);
			map
		});

		let mut ignores_by_user = HashMap::new();

		let futures = FuturesUnordered::new();

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

			if keywords.is_empty() {
				continue;
			}

			let ctx = ctx.clone();
			futures.push(highlighting::notify_keywords(
				ctx,
				message.clone(),
				keywords,
				ignores.clone(),
				user_id,
				guild_id,
			));
		}

		futures.for_each(|_| async move {}).await;

		Ok(())
	}
	.await;

	if let Err(e) = res.context("Failed to handle keywords") {
		tracing::error!("{:?}", e);
	}
}

/// Handles a slash [`command`](commands).
async fn handle_command(ctx: Context, command: Command) {
	let name = command.data.name.clone();
	let channel_id = command.channel_id;
	let user_id = command.user.id;

	let span = info_span!(
		parent: None,
		"interaction_create",
		interaction_id = %command.id,
		author_id = %user_id,
		channel_id = %channel_id,
		guild_id = ?command.guild_id,
	);

	let _entered = span.enter();

	let result = {
		use std::future::Future;

		use commands::*;
		use tokio::task::JoinHandle;

		fn spawn_command<Fut>(
			ctx: Context,
			command: Command,
			f: fn(Context, Command) -> Fut,
		) -> JoinHandle<Result<()>>
		where
			Fut: Future<Output = Result<()>> + Send + 'static,
		{
			let parent = Span::current();
			tokio::spawn(async move {
				let span = info_span!(parent: &parent, "spawn_command");
				let _entered = span.enter();
				f(ctx, command).await
			})
		}

		let ctx = ctx.clone();
		let command = command.clone();

		match &*name {
			"add" => spawn_command(ctx, command, add),
			"remove" => spawn_command(ctx, command, remove),
			"mute" => spawn_command(ctx, command, mute),
			"unmute" => spawn_command(ctx, command, unmute),
			"ignore" => spawn_command(ctx, command, ignore),
			"unignore" => spawn_command(ctx, command, unignore),
			"block" => spawn_command(ctx, command, block),
			"unblock" => spawn_command(ctx, command, unblock),
			"remove-server" => spawn_command(ctx, command, remove_server),
			"keywords" => spawn_command(ctx, command, keywords),
			"mutes" => spawn_command(ctx, command, mutes),
			"ignores" => spawn_command(ctx, command, ignores),
			"blocks" => spawn_command(ctx, command, blocks),
			"opt-out" => spawn_command(ctx, command, opt_out),
			"opt-in" => spawn_command(ctx, command, opt_in),
			"help" => spawn_command(ctx, command, help),
			"ping" => spawn_command(ctx, command, ping),
			"about" => spawn_command(ctx, command, about),
			_ => {
				let err =
					anyhow::anyhow!("Unknown slash command received: {}", name);

				tokio::spawn(async move { Err(err) })
			}
		}
		.await
		.map_err(anyhow::Error::from)
		.and_then(|r| r)
	};

	if let Err(e) = result {
		tracing::debug!("Reporting failure to user");
		const BUG_REPORT_PROMPT: &str =
			"I would appreciate if you could take a minute to [file a bug report]\
			(https://github.com/ThatsNoMoon/highlights/issues/new?template=bug_report.md) \
			so I can work on fixing this! Please include the interaction ID \
			below in your report. Thanks!";

		let embed = {
			let mut embed = CreateEmbed::default();
			embed
				.color(ERROR_COLOR)
				.title("An error occurred running that command :(")
				.description({
					let mut e = format!("{:#}", e);
					if e.len() > 2000 {
						e.truncate(2000);
						e.push_str("...")
					}
					e
				})
				.field("Create a bug report", BUG_REPORT_PROMPT, true)
				.footer(|f| f.text(format!("Interaction ID: {}", command.id)));
			embed
		};

		let response_result = command
			.create_interaction_response(&ctx, |r| {
				r.interaction_response_data(|d| {
					d.flags(ResponseFlags::EPHEMERAL).add_embed(embed.clone())
				})
			})
			.await;

		const INTERACTION_ACKNOWLEDGED: isize = 40060;

		let response_result = match response_result {
			Ok(_) => Ok(()),
			Err(SerenityError::Http(e))
				if matches!(
					&*e,
					HttpError::UnsuccessfulRequest(ErrorResponse {
						error: DiscordJsonError {
							code: INTERACTION_ACKNOWLEDGED,
							..
						},
						..
					},)
				) =>
			{
				command
					.create_followup_message(&ctx, |c| {
						c.flags(ResponseFlags::EPHEMERAL).add_embed(embed)
					})
					.await
					.context("Failed to send failure followup")
					.map(drop)
			}
			Err(e) => Err(e).context("Failed to send failure response"),
		};

		tracing::error!("{:?}", e);

		if let Err(e) = response_result {
			tracing::error!("{:?}", e);
		}
	}

	if let Err(e) = highlighting::check_notify_user_state(&ctx, &command)
		.await
		.context("Failed to check and notify user state")
	{
		tracing::error!("{:?}", e);
	}
}

/// [`TypeMapKey`] to store a reference to the [`ShardManager`] for retrieving
/// latency.
struct Shards;

impl TypeMapKey for Shards {
	type Value = Arc<Mutex<ShardManager>>;
}

/// Initializes the Discord client.
pub(crate) async fn init() -> Result<()> {
	let mut client = Client::builder(
		&settings().bot.token,
		GatewayIntents::DIRECT_MESSAGES
			| GatewayIntents::GUILD_MESSAGE_REACTIONS
			| GatewayIntents::GUILD_MESSAGES
			| GatewayIntents::GUILDS
			| GatewayIntents::GUILD_MEMBERS,
	)
	.event_handler(Handler)
	.application_id(settings().bot.application_id)
	.await
	.context("Failed to create client")?;

	{
		let mut data = client.data.write().await;

		data.insert::<CachedMessages>(HashMap::new());
		data.insert::<Shards>(client.shard_manager.clone());
	}

	client.start().await.context("Failed to run client")?;

	Ok(())
}
