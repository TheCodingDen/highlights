use once_cell::sync::Lazy;
use serenity::{
	client::Context,
	model::{
		channel::{ChannelType, GuildChannel, Message},
		id::ChannelId,
	},
};

use std::{collections::HashMap, convert::TryInto};

use crate::{
	db::{Follow, Keyword},
	util::{error, question},
	Error, global::MAX_KEYWORDS,
};

pub async fn add(
	ctx: &Context,
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

	let keyword = Keyword {
		keyword: args.to_owned(),
		user_id: message.author.id.0.try_into().unwrap(),
		server_id: guild_id.0.try_into().unwrap(),
	};

	{
		let keyword_count =
			Keyword::user_keyword_count(message.author.id).await?;

		if keyword_count >= MAX_KEYWORDS {
			static MSG: Lazy<String, fn() -> String> = Lazy::new(|| {
				format!("You can't create more than {} keywords!", MAX_KEYWORDS)
			});

			return error(ctx, message, MSG.as_str()).await;
		}
	}

	{
		let exists = keyword.clone().exists().await?;

		if exists {
			return error(ctx, message, "You already added that keyword!")
				.await;
		}
	}

	keyword.insert().await?;

	message.react(ctx, '✅').await?;

	Ok(())
}

pub async fn follow(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	fn get_channel_from_arg<'c>(
		channels: &HashMap<&ChannelId, &'c GuildChannel>,
		arg: &str,
	) -> Option<&'c GuildChannel> {
		if let Ok(id) = arg.parse::<u64>() {
			return channels.get(&ChannelId(id)).copied();
		}

		if let Some(id) = arg
			.strip_prefix("<#")
			.and_then(|arg| arg.strip_suffix(">"))
			.and_then(|arg| arg.parse::<u64>().ok())
		{
			return channels.get(&ChannelId(id)).copied();
		}

		let mut iter = channels
			.iter()
			.map(|(_, channel)| channel)
			.filter(|channel| channel.name.as_str().eq_ignore_ascii_case(arg));

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
	let _user_roles = member.roles.as_slice();

	let guild = ctx.cache.guild(guild_id).await.unwrap();
	let channels = guild
		.channels
		.iter()
		.filter(|(_, channel)| matches!(channel.kind, ChannelType::Text))
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
				if !channel
					.permissions_for_user(ctx, user_id)
					.await?
					.read_messages()
				{
					forbidden.push(arg);
				} else {
					let user_id: i64 = user_id.0.try_into().unwrap();
					let channel_id: i64 = channel.id.0.try_into().unwrap();

					let follow = Follow {
						user_id,
						channel_id,
					};

					if follow.clone().exists().await? {
						already_followed.push(format!("<#{}>", channel_id));
					} else {
						followed.push(format!("<#{}>", channel.id));
						follow.insert().await?;
					}
				}
			}
		}
	}

	let mut msg = String::new();

	if !followed.is_empty() {
		msg.push_str("Followed channels: ");
		msg.push_str(&followed.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !already_followed.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Channels already followed: ");
		msg.push_str(&already_followed.join(", "));

		message.react(ctx, '❌').await?;
	}

	if !not_found.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Couldn't find channels: ");
		msg.push_str(&not_found.join(", "));

		message.react(ctx, '❓').await?;
	}

	if !forbidden.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("Unable to follow channels: ");
		msg.push_str(&forbidden.join(", "));

		message.react(ctx, '❌').await?;
	}

	message
		.channel_id
		.send_message(ctx, |m| m.content(msg))
		.await?;

	Ok(())
}
