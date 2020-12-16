// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

#![type_length_limit = "10000000"]

mod commands;

pub mod db;
use db::{Ignore, Keyword, UserState};

mod error;
pub use error::Error;

pub mod global;
use global::{bot_mention, bot_nick_mention, init_mentions};

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
		gateway::{Activity, Ready},
		id::UserId,
	},
};
use tokio::task;

use std::{collections::HashMap, convert::TryInto, env};

struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
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

	async fn ready(&self, ctx: Context, ready: Ready) {
		init_mentions(ready.user.id);

		let username = ctx.cache.current_user_field(|u| u.name.clone()).await;

		ctx.set_activity(Activity::listening(&format!("@{} help", username)))
			.await;

		log::info!("Ready to highlight!");
	}
}

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
		"remove-server" => commands::remove_server(ctx, message, args).await,
		"keywords" => commands::keywords(ctx, message, args).await,
		"mutes" => commands::mutes(ctx, message, args).await,
		"ignores" => commands::ignores(ctx, message, args).await,
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

async fn handle_keywords(
	ctx: &Context,
	message: &Message,
) -> Result<(), Error> {
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
			));
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() {
	let _ = dotenv::dotenv();

	reporting::init();

	let token = env::var("HIGHLIGHTS_DISCORD_TOKEN")
		.expect("HIGHLIGHTS_DISCORD_TOKEN must be set");

	db::init();

	monitoring::init();

	let mut client = Client::builder(token)
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
