mod commands;
pub mod util;

pub mod db;
use db::{Follow, Keyword};

use automate::{
	gateway::{Message, MessageCreateDispatch, ReadyDispatch},
	http::{CreateMessage, Recipient},
	listener, stateless, Configuration, Context, Error, ShardManager,
	Snowflake,
};
use once_cell::sync::{Lazy, OnceCell};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use std::{convert::TryInto, env};

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

static OWNER_ID: Lazy<Snowflake> = Lazy::new(|| {
	const DEFAULT: u64 = 257711607096803328;

	let id = env::var("DISCORD_OWNER_ID")
		.ok()
		.and_then(|s| s.parse().ok())
		.unwrap_or(DEFAULT);

	Snowflake(id)
});

static LOG_CHANNEL_ID: OnceCell<Snowflake> = OnceCell::new();

fn log_channel_id() -> Snowflake {
	*LOG_CHANNEL_ID
		.get()
		.expect("Log channel id was not initialized")
}

pub async fn question(
	ctx: &mut Context,
	message: &Message,
) -> Result<(), Error> {
	ctx.create_reaction(message.channel_id, message.id, &"❓")
		.await
}

pub async fn error<S: Into<String>>(
	ctx: &mut Context,
	message: &Message,
	response: S,
) -> Result<(), Error> {
	let _ = ctx
		.create_reaction(message.channel_id, message.id, &"❌")
		.await;

	ctx.create_message(
		message.channel_id,
		CreateMessage {
			content: Some(response.into()),
			..Default::default()
		},
	)
	.await?;

	Ok(())
}

#[listener]
async fn ready_listener(
	ctx: &mut Context,
	data: &ReadyDispatch,
) -> Result<(), Error> {
	let id = data.user.id.0;

	BOT_MENTION.set(format!("<@{}>", id)).unwrap();
	BOT_NICK_MENTION.set(format!("<@!{}>", id)).unwrap();

	let log_channel_id = match ctx
		.create_dm::<Message>(Recipient {
			recipient_id: *OWNER_ID,
		})
		.await
	{
		Ok(channel) => channel.id,
		Err(_) => {
			ctx.channel(*OWNER_ID)
				.await
				.expect("Failed to get owner ID DM or text channel")
				.id
		}
	};

	LOG_CHANNEL_ID.set(log_channel_id).unwrap();

	Ok(())
}

#[listener]
async fn message_listener(
	ctx: &mut Context,
	data: &MessageCreateDispatch,
) -> Result<(), Error> {
	let message = &data.0;
	if let Some(true) = message.author.bot {
		return Ok(());
	}

	let content = message.content.as_str();

	let result = match content
		.strip_prefix(bot_mention())
		.or_else(|| content.strip_prefix(bot_nick_mention()))
	{
		Some(command_content) => {
			handle_command(ctx, message, command_content.trim()).await
		}
		None => handle_keywords(ctx, message, content).await,
	};

	if let Err(e) = &result {
		let msg = format!(
			"Error in {} by {}: {}",
			message.channel_id, message.author.id, e
		);
		let _ = ctx
			.create_message(
				log_channel_id(),
				CreateMessage {
					content: Some(msg),
					..CreateMessage::default()
				},
			)
			.await;
	}

	result
}

async fn handle_command(
	ctx: &mut Context,
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
	ctx: &mut Context,
	message: &Message,
	content: &str,
) -> Result<(), Error> {
	let guild_id = match message.guild_id {
		Some(id) => id,
		None => return Ok(()),
	};

	let channel_id = message.channel_id;

	let keywords = Keyword::get_relevant_keywords(guild_id, channel_id)
		.await
		.map_err(|err| Error {
		msg: format!("Failed to get keywords: {}", err),
	})?;

	for result in keywords {
		let user_id: u64 = result.user_id.try_into().unwrap();

		if content.contains(&result.keyword) {
			let channel = ctx
				.create_dm::<Message>(Recipient {
					recipient_id: Snowflake(user_id),
				})
				.await?;
			ctx.create_message(
				channel,
				CreateMessage {
					content: Some(format!(
						"Your keyword {} was seen in <#{}>: {}",
						result.keyword, channel_id, content
					)),
					..Default::default()
				},
			)
			.await?;
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() {
	let config = Configuration::from_env("DISCORD_TOKEN")
		.register(stateless!(message_listener))
		.register(stateless!(ready_listener));

	let pool = {
		let manager = SqliteConnectionManager::file("data.db").with_flags(
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		);
		Pool::new(manager).expect("Failed to open database pool")
	};

	POOL.set(pool).unwrap();

	Follow::create_table();
	Keyword::create_table();

	ShardManager::with_config(config)
		.await
		.auto_setup()
		.launch()
		.await;
}
