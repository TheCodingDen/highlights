use automate::{
	gateway::{Message, MessageCreateDispatch, ReadyDispatch},
	http::{CreateMessage, Recipient},
	listener, stateless, Configuration, Context, Error, ShardManager,
	Snowflake,
};
use futures::stream::StreamExt;
use once_cell::sync::{OnceCell, Lazy};
use sqlx::{
	query,
	sqlite::{SqliteConnectOptions, SqlitePool},
};

use std::{convert::TryInto, env, str::FromStr};

mod commands;
pub mod util;

pub const MAX_KEYWORDS: i64 = 100;

static POOL: OnceCell<SqlitePool> = OnceCell::new();

pub fn pool() -> &'static SqlitePool {
	POOL.get().expect("Database pool was not initialized")
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

	let id = env::var("DISCORD_OWNER_ID").and_then(|s| s.parse().ok()).unwrap_or(DEFAULT);

	Snowflake(id)
});

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

	return Ok(());
}

#[listener]
async fn ready_listener(
	_: &mut Context,
	data: &ReadyDispatch,
) -> Result<(), Error> {
	let id = data.user.id.0;

	BOT_MENTION.set(format!("<@{}>", id)).unwrap();
	BOT_NICK_MENTION.set(format!("<@!{}>", id)).unwrap();

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
	let guild_id: i64 = match message.guild_id {
		Some(id) => id.0.try_into().unwrap(),
		None => return Ok(()),
	};

	let channel_id: i64 = message.channel_id.0.try_into().unwrap();

	let mut rows = query!(
		"SELECT keywords.keyword, keywords.user_id
		FROM keywords
		INNER JOIN follows
		ON keywords.user_id = follows.user_id
		WHERE keywords.server_id = ? AND follows.channel_id = ?",
		guild_id,
		channel_id,
	)
	.fetch(pool());

	while let Some(result) = rows.next().await {
		let result = result.map_err(|e| Error {
			msg: format!("Failed to get keyword: {}", e),
		})?;

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
		let url = env::var("DATABASE_URL")
			.unwrap_or(String::from("sqlite://./data.db"));
		let db_options = SqliteConnectOptions::from_str(&url)
			.expect("Failed to parse connection options")
			.create_if_missing(true);

		SqlitePool::connect_with(db_options)
			.await
			.expect("Failed to open database pool")
	};

	query!("CREATE TABLE IF NOT EXISTS follows (user_id INTEGER NOT NULL, channel_id INTEGER NOT NULL, PRIMARY KEY (user_id, channel_id))")
		.execute(&pool)
		.await
		.expect("Failed to create follows table");

	query!("CREATE TABLE IF NOT EXISTS keywords (keyword TEXT NOT NULL, user_id INTEGER NOT NULL, server_id INTEGER NOT NULL, PRIMARY KEY (keyword, user_id, server_id))")
		.execute(&pool)
		.await
		.expect("Failed to create keywords table");

	POOL.set(pool).unwrap();

	ShardManager::with_config(config)
		.await
		.auto_setup()
		.launch()
		.await;
}
