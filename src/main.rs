// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

mod commands;

pub mod db;
use db::Keyword;

mod error;
pub use error::Error;

pub mod global;
use global::{bot_mention, bot_nick_mention, init_mentions};

pub mod monitoring;

pub mod reporting;

#[macro_use]
pub mod util;
use util::{error, notify_keyword, question};

use serenity::{
	client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler},
	model::{
		channel::Message,
		gateway::{Activity, Ready},
		id::UserId,
	},
};
use tokio::task;

use std::{convert::TryInto, env};

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
				handle_command(&ctx, &message, command_content.trim()).await
			}
			None => handle_keywords(&ctx, &message, content).await,
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
		let mut iter = content.splitn(2, ' ');

		let command = match iter.next() {
			Some(c) => c,
			None => return question(ctx, message).await,
		};

		let args = iter.next().map(|s| s.trim()).unwrap_or("");

		(command, args)
	};

	let result = match command {
		"add" => commands::add(ctx, message, args).await,
		"remove" => commands::remove(ctx, message, args).await,
		"mute" => commands::mute(ctx, message, args).await,
		"unmute" => commands::unmute(ctx, message, args).await,
		"remove-server" => commands::remove_server(ctx, message, args).await,
		"keywords" => commands::keywords(ctx, message, args).await,
		"mutes" => commands::mutes(ctx, message, args).await,
		"help" => commands::help(ctx, message, args).await,
		"about" => commands::about(ctx, message, args).await,
		_ => question(ctx, message).await,
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
	content: &str,
) -> Result<(), Error> {
	let guild_id = match message.guild_id {
		Some(id) => id,
		None => return Ok(()),
	};

	let channel_id = message.channel_id;

	let keywords =
		Keyword::get_relevant_keywords(guild_id, channel_id, message.author.id)
			.await?;

	for keyword in keywords {
		let start = {
			let mut fragments = regex!(r"[^a-zA-Z0-9]").split(content);

			let substring = match fragments
				.find(|frag| keyword.keyword.eq_ignore_ascii_case(frag))
			{
				Some(s) => s,
				None => continue,
			};

			let substring_start = substring.as_ptr() as usize;
			let content_start = content.as_ptr() as usize;
			let substring_index = substring_start - content_start;

			substring_index
		};
		let end = start + keyword.keyword.len();

		let user_id = UserId(keyword.user_id.try_into().unwrap());
		let channel = match ctx.cache.guild_channel(channel_id).await {
			Some(c) => c,
			None => {
				log::error!("Channel not cached: {}", channel_id);
				return Ok(());
			}
		};

		if channel
			.permissions_for_user(ctx, user_id)
			.await?
			.read_messages()
		{
			let ctx = ctx.clone();
			task::spawn(notify_keyword(
				ctx,
				message.clone(),
				start..end,
				keyword,
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

	let mut client = Client::new(token)
		.event_handler(Handler)
		.intents(
			GatewayIntents::DIRECT_MESSAGES
				| GatewayIntents::GUILD_MESSAGES
				| GatewayIntents::GUILDS
				| GatewayIntents::GUILD_MEMBERS,
		)
		.await
		.expect("Failed to create client");

	client.start().await.expect("Failed to run client");
}
