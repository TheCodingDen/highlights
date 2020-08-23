mod commands;

pub mod db;
use db::Keyword;

pub mod global;
use global::{
	bot_mention, bot_nick_mention, init_log_channel_id, init_mentions,
	log_channel_id,
};

pub mod monitoring;

pub mod util;
use util::{error, notify_keyword, question, report_error};

use rusqlite::Error as RusqliteError;
use serenity::{model::prelude::*, prelude::*};
use tokio::task;

use std::{convert::TryInto, env, error::Error as StdError, fmt::Display};

#[derive(Debug)]
struct SimpleError(String);

impl Display for SimpleError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		Display::fmt(&self.0, f)
	}
}

impl StdError for SimpleError {}

#[derive(Debug)]
pub struct Error(Box<dyn StdError + Send + Sync + 'static>);

impl From<SerenityError> for Error {
	fn from(e: SerenityError) -> Self {
		Self(Box::new(e))
	}
}

impl From<RusqliteError> for Error {
	fn from(e: RusqliteError) -> Self {
		Self(Box::new(e))
	}
}

impl From<String> for Error {
	fn from(e: String) -> Self {
		Self(Box::new(SimpleError(e)))
	}
}

impl From<&'_ str> for Error {
	fn from(e: &str) -> Self {
		Self(Box::new(SimpleError(e.to_owned())))
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		Display::fmt(&self.0, f)
	}
}

impl StdError for Error {}
impl StdError for &'_ Error {}

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
			report_error(&ctx, message.channel_id, message.author.id, e).await;
		}
	}

	async fn ready(&self, ctx: Context, ready: Ready) {
		init_mentions(ready.user.id);

		init_log_channel_id(&ctx).await;
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
		"follow" => commands::follow(ctx, message, args).await,
		"remove" => commands::remove(ctx, message, args).await,
		"removeserver" => commands::remove_server(ctx, message, args).await,
		"unfollow" => commands::unfollow(ctx, message, args).await,
		"keywords" => commands::keywords(ctx, message, args).await,
		"follows" => commands::follows(ctx, message, args).await,
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
		let start = match content.to_lowercase().find(&keyword.keyword) {
			Some(i) => i,
			None => continue,
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

	env_logger::from_env(
		env_logger::Env::new()
			.filter("HIGHLIGHTS_LOG_FILTER")
			.write_style("HIGHLIGHTS_LOG_STYLE"),
	)
	.init();

	let token = env::var("HIGHLIGHTS_DISCORD_TOKEN")
		.expect("HIGHLIGHTS_DISCORD_TOKEN must be set");

	db::init();

	monitoring::init();

	let mut client = Client::new(token)
		.event_handler(Handler)
		.await
		.expect("Failed to create client");

	client.start().await.expect("Failed to run client");
}
