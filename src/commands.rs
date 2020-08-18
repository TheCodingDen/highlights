use automate::{
	gateway::{Channel, ChannelType, Guild, Message},
	http::CreateMessage,
	Context, Error, Snowflake,
};
use futures::stream::StreamExt;
use once_cell::sync::Lazy;
use sqlx::{query, FromRow};

use std::{convert::TryInto, collections::HashMap};

use crate::{
	error, pool, question, util::member_can_read_channel, MAX_KEYWORDS,
};

pub async fn add(
	ctx: &mut Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let guild_id = match message.guild_id {
		None => {
			return error(
				ctx,
				message,
				"You must run this command in a server!",
			)
			.await
		}
		Some(id) => id,
	};

	if args == "" {
		return question(ctx, message).await;
	}

	if args.len() <= 2 {
		return error(
			ctx,
			message,
			"You can't highlight keywords shorter than 3 characters!",
		)
		.await;
	}

	let user_id: i64 = message.author.id.0.try_into().unwrap();

	let guild_id: i64 = guild_id.0.try_into().unwrap();

	{
		let mut rows =
			sqlx::query("SELECT COUNT(*) FROM keywords WHERE user_id = ?")
				.bind(user_id)
				.fetch(pool());

		let row = rows
			.next()
			.await
			.ok_or_else(|| Error {
				msg: String::from("No keyword count"),
			})?
			.map_err(|e| Error {
				msg: format!("Failed to get keywords: {}", e),
			})?;

		let (keyword_count,) = <(i64,)>::from_row(&row).map_err(|e| Error {
			msg: format!("Failed to get i64 from row: {}", e),
		})?;

		if keyword_count >= MAX_KEYWORDS {
			static MSG: Lazy<String, fn() -> String> = Lazy::new(|| {
				format!("You can't create more than {} keywords!", MAX_KEYWORDS)
			});

			return error(ctx, message, MSG.as_str()).await;
		}
	}

	{
		let existing = sqlx::query!(
			"SELECT keyword FROM keywords WHERE keyword = ? AND user_id = ? AND server_id = ?",
			args,
			user_id,
			guild_id
		)
		.fetch_optional(pool())
		.await
		.map_err(|e| Error { msg: format!("Failed to query existing keyword: {}", e) })?;

		if let Some(_) = existing {
			return error(ctx, message, "You already added that keyword!")
				.await;
		}
	}

	sqlx::query!(
		"INSERT INTO keywords (keyword, user_id, server_id) VALUES (?, ?, ?)",
		args,
		user_id,
		guild_id,
	)
	.execute(pool())
	.await
	.map_err(|e| Error {
		msg: format!("Failed to insert keyword: {}", e),
	})?;

	ctx.create_reaction(message.channel_id, message.id, &"✅")
		.await?;

	Ok(())
}

pub async fn follow(
	ctx: &mut Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	fn get_channel_from_arg<'c>(
		channels: &'c HashMap<Snowflake, Channel>,
		arg: &str,
	) -> Option<&'c Channel> {
		if let Ok(id) = arg.parse::<u64>() {
			return channels.get(&Snowflake(id));
		}

		if let Some(id) = arg
			.strip_prefix("<#")
			.and_then(|arg| arg.strip_suffix(">"))
			.and_then(|arg| arg.parse::<u64>().ok())
		{
			return channels.get(&Snowflake(id));
		}

		let mut iter = channels.iter().map(|(_, channel)| channel).filter(|channel| {
				channel.name.as_ref().unwrap().eq_ignore_ascii_case(arg)
		});

		if let Some(first) = iter.next() {
			if let None = iter.next() {
				return Some(first);
			}
		}

		None
	}

	let guild_id = match message.guild_id {
		None => {
			return error(
				ctx,
				message,
				"You must run this command in a server!",
			)
			.await
		}
		Some(id) => id,
	};

	if args == "" {
		return question(ctx, message).await;
	}

	let user_id = message.author.id;
	let member = message.member.as_ref().unwrap();
	let user_roles = member.roles.as_slice();

	let guild = ctx.guild(guild_id).await.unwrap();
	let channels = ctx
		.channels(guild_id)
		.await?
		.into_iter()
		.filter(|channel| matches!(channel._type, ChannelType::GuildText))
		.map(|channel| (channel.id, channel))
		.collect::<HashMap<_, _>>();

	let mut followed = vec![];
	let mut already_followed = vec![];
	let mut not_found = vec![];
	let mut forbidden = vec![];

	for arg in args.split_whitespace() {
		let channel = get_channel_from_arg(&channels, arg);

		match channel {
			None => not_found.push(arg),
			Some(channel) => {
				if !member_can_read_channel(
					user_id, user_roles, &channel, &guild,
				) {
					forbidden.push(arg);
				} else {
					let user_id: i64 = user_id.0.try_into().unwrap();
					let channel_id: i64 = channel.id.0.try_into().unwrap();

					let existing = query!(
						"SELECT channel_id FROM follows WHERE user_id = ? AND channel_id = ?",
						user_id,
						channel_id,
					)
					.fetch_optional(pool())
					.await
					.map_err(|e| Error {
						msg: format!("Failed to query existing follow: {}", e),
					})?;

					if let Some(_) = existing {
						already_followed.push(format!("<#{}>", channel_id));
					} else {
						followed.push(format!("<#{}>", channel.id));

						query!(
							"INSERT INTO follows (user_id, channel_id) VALUES (?, ?)",
							user_id,
							channel_id,
						)
						.execute(pool())
						.await
						.map_err(|e| Error {
							msg: format!("Failed to insert keyword: {}", e),
						})?;
					}
				}
			}
		}
	}

	let mut msg = String::new();

	if !followed.is_empty() {
		msg.push_str("Followed channels: ");
		msg.push_str(&followed.join(", "));

		ctx.create_reaction(message.channel_id, message.id, &"✅")
			.await?;
	}

	if !already_followed.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Channels already followed: ");
		msg.push_str(&already_followed.join(", "));

		ctx.create_reaction(message.channel_id, message.id, &"❌")
			.await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find channels: ");
		msg.push_str(&not_found.join(", "));

		ctx.create_reaction(message.channel_id, message.id, &"❓")
			.await?;
	}

	if !forbidden.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Unable to follow channels: ");
		msg.push_str(&forbidden.join(", "));

		ctx.create_reaction(message.channel_id, message.id, &"❌")
			.await?;
	}

	ctx.create_message(
		message.channel_id,
		CreateMessage {
			content: Some(msg),
			..Default::default()
		},
	)
	.await?;

	Ok(())
}
