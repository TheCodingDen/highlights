use automate::{
	gateway::{Channel, ChannelType, Message},
	http::CreateMessage,
	Context, Error, Snowflake,
};
use once_cell::sync::Lazy;

use std::{collections::HashMap, convert::TryInto};

use crate::{
	db::{Follow, Keyword},
	error, question,
	util::member_can_read_channel,
	MAX_KEYWORDS,
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

	let keyword = Keyword {
		keyword: args.to_owned(),
		user_id,
		server_id: guild_id,
	};

	{
		let keyword_count = Keyword::user_keyword_count(user_id)
			.await
			.map_err(|err| Error {
				msg: format!("Failed to count user keywords: {}", err),
			})?;

		if keyword_count >= MAX_KEYWORDS {
			static MSG: Lazy<String, fn() -> String> = Lazy::new(|| {
				format!("You can't create more than {} keywords!", MAX_KEYWORDS)
			});

			return error(ctx, message, MSG.as_str()).await;
		}
	}

	{
		let exists = keyword.clone().exists().await.map_err(|err| Error {
			msg: format!("Failed to check for keyword existence: {}", err),
		})?;

		if exists {
			return error(ctx, message, "You already added that keyword!")
				.await;
		}
	}

	keyword.insert().await.map_err(|e| Error {
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

		let mut iter =
			channels
				.iter()
				.map(|(_, channel)| channel)
				.filter(|channel| {
					channel.name.as_ref().unwrap().eq_ignore_ascii_case(arg)
				});

		if let Some(first) = iter.next() {
			if iter.next().is_none() {
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

					let follow = Follow {
						user_id,
						channel_id,
					};

					let exists = {
						follow.clone().exists().await.map_err(|err| Error {
							msg: format!(
								"Failed to check for follow existence: {}",
								err
							),
						})?
					};

					if exists {
						already_followed.push(format!("<#{}>", channel_id));
					} else {
						followed.push(format!("<#{}>", channel.id));
						follow.insert().await.map_err(|e| Error {
							msg: format!("Failed to insert follow: {}", e),
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
