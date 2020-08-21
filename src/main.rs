mod commands;

pub mod util;
use util::{get_channel_for_owner_id, report_error};

pub mod db;
use db::{Follow, Keyword};

use once_cell::sync::{Lazy, OnceCell};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Error as RusqliteError, OpenFlags};
use serenity::{model::prelude::*, prelude::*};

use std::{convert::TryInto, env, error::Error as StdError, fmt::Display};

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

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		Display::fmt(&self.0, f)
	}
}

pub const MAX_KEYWORDS: u32 = 100;

static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

static BOT_MENTION: OnceCell<String> = OnceCell::new();
static BOT_NICK_MENTION: OnceCell<String> = OnceCell::new();

fn bot_mention() -> &'static str {
	BOT_MENTION
		.get()
		.expect("Bot mention was not initialized")
		.as_str()
}

fn bot_nick_mention() -> &'static str {
	BOT_NICK_MENTION
		.get()
		.expect("Bot nick mention was not initialized")
		.as_str()
}

pub static OWNER_ID: Lazy<u64> = Lazy::new(|| {
	const DEFAULT: u64 = 257711607096803328;

	env::var("DISCORD_OWNER_ID")
		.ok()
		.and_then(|s| s.parse().ok())
		.unwrap_or(DEFAULT)
});

static LOG_CHANNEL_ID: OnceCell<ChannelId> = OnceCell::new();

fn log_channel_id() -> ChannelId {
	*LOG_CHANNEL_ID
		.get()
		.expect("Log channel id was not initialized")
}

pub async fn question(ctx: &Context, message: &Message) -> Result<(), Error> {
	message.react(ctx, '❓').await?;

	Ok(())
}

pub async fn error<S: Display>(
	ctx: &Context,
	message: &Message,
	response: S,
) -> Result<(), Error> {
	let _ = message.react(ctx, '❌').await;

	message
		.channel_id
		.send_message(ctx, |m| m.content(response))
		.await?;

	Ok(())
}

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
		let id = ready.user.id;

		BOT_MENTION.set(format!("<@{}>", id)).unwrap();
		BOT_NICK_MENTION.set(format!("<@!{}>", id)).unwrap();

		LOG_CHANNEL_ID
			.set(get_channel_for_owner_id(&ctx).await)
			.unwrap();
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

	let keywords = Keyword::get_relevant_keywords(guild_id, channel_id).await?;

	for result in keywords {
		if !content.contains(&result.keyword) {
			continue;
		}

		let user_id = UserId(result.user_id.try_into().unwrap());
		let channel = match ctx.cache.guild_channel(channel_id).await {
			Some(c) => c,
			None => {
				log::error!("Channel not cached: {}", channel_id);
				return Ok(());
			}
		};

		if content.contains(&result.keyword)
			&& channel
				.permissions_for_user(ctx, user_id)
				.await?
				.read_messages()
		{
			user_id
				.create_dm_channel(ctx)
				.await?
				.send_message(ctx, |m| {
					m.content(format!(
						"Your keyword {} was seen in <#{}>: {}",
						result.keyword, channel_id, content
					))
				})
				.await?;
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() {
	let _ = dotenv::dotenv();

	env_logger::init();

	let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");

	let pool = {
		let manager = SqliteConnectionManager::file("data.db").with_flags(
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		);
		Pool::new(manager).expect("Failed to open database pool")
	};

	POOL.set(pool).unwrap();

	Follow::create_table();
	Keyword::create_table();

	let mut client = Client::new(token)
		.event_handler(Handler)
		.await
		.expect("Failed to create client");

	client.start().await.expect("Failed to run client");
}
