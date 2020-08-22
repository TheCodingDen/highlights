use once_cell::sync::Lazy;
use serenity::{
	client::Context,
	model::{
		channel::{ChannelType, GuildChannel, Message},
		id::ChannelId,
		Permissions,
	},
};

use std::{collections::HashMap, convert::TryInto};

use crate::{
	db::{Follow, Keyword},
	global::{EMBED_COLOR, MAX_KEYWORDS},
	util::{error, question},
	Error,
};

macro_rules! check_guild {
	($ctx:expr, $message:expr) => {{
		match $message.guild_id {
			None => {
				return error(
					$ctx,
					$message,
					"You must run this command in a server!",
				)
				.await
				}
			Some(id) => id,
			}
		}};
}

macro_rules! check_empty_args {
	($args:expr, $ctx:expr, $message:expr) => {{
		if $args == "" {
			return question($ctx, $message).await;
			}
		}};
}

pub async fn add(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	if args.len() <= 2 {
		return error(
			ctx,
			message,
			"You can't highlight keywords shorter than 3 characters!",
		)
		.await;
	}

	let keyword = Keyword {
		keyword: args.to_lowercase(),
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

	if keyword.clone().exists().await? {
		return error(ctx, message, "You already added that keyword!").await;
	}

	keyword.insert().await?;

	message.react(ctx, '✅').await?;

	Ok(())
}

pub async fn remove(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	let keyword = Keyword {
		keyword: args.to_lowercase(),
		user_id: message.author.id.0.try_into().unwrap(),
		server_id: guild_id.0.try_into().unwrap(),
	};

	if !keyword.clone().exists().await? {
		return error(ctx, message, "You haven't added that keyword!").await;
	}

	keyword.delete().await?;

	message.react(ctx, '✅').await?;

	Ok(())
}

pub async fn follow(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	let user_id = message.author.id;
	let self_id = ctx.cache.current_user_id().await;

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
				let user_can_read = channel
					.permissions_for_user(ctx, user_id)
					.await?
					.read_messages();
				let self_can_read = channel
					.permissions_for_user(ctx, self_id)
					.await?
					.read_messages();

				if !user_can_read || !self_can_read {
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

	let mut msg = String::with_capacity(45);

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

		if already_followed.is_empty() {
			message.react(ctx, '❌').await?;
		}
	}

	message.channel_id.say(ctx, msg).await?;

	Ok(())
}

pub async fn unfollow(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	let user_id = message.author.id;

	let guild = ctx.cache.guild(guild_id).await.unwrap();
	let channels = guild
		.channels
		.iter()
		.filter(|(_, channel)| matches!(channel.kind, ChannelType::Text))
		.collect::<HashMap<_, _>>();

	let mut unfollowed = vec![];
	let mut not_followed = vec![];
	let mut not_found = vec![];

	for arg in args.split_whitespace() {
		let channel = get_channel_from_arg(&channels, arg);

		match channel {
			None => not_found.push(arg),
			Some(channel) => {
				let user_id: i64 = user_id.0.try_into().unwrap();
				let channel_id: i64 = channel.id.0.try_into().unwrap();

				let follow = Follow {
					user_id,
					channel_id,
				};

				if !follow.clone().exists().await? {
					not_followed.push(format!("<#{}>", channel_id));
				} else {
					unfollowed.push(format!("<#{}>", channel.id));
					follow.delete().await?;
				}
			}
		}
	}

	let mut msg = String::with_capacity(50);

	if !unfollowed.is_empty() {
		msg.push_str("Unfollowed channels: ");
		msg.push_str(&unfollowed.join(", "));

		message.react(ctx, '✅').await?;
	}

	if !not_followed.is_empty() {
		if !msg.is_empty() {
			msg.push('\n');
		}
		msg.push_str("You weren't following channels: ");
		msg.push_str(&not_followed.join(", "));

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

	message.channel_id.say(ctx, msg).await?;

	Ok(())
}

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

pub async fn help(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	struct CommandInfo {
		name: &'static str,
		short_desc: &'static str,
		long_desc: String,
	}

	let username = ctx.cache.current_user_field(|u| u.name.clone()).await;

	let commands = [
		CommandInfo {
			name: "add",
			short_desc: "Add a keyword to highlight in the current server",
			long_desc: format!(
"Use `@{name} add [keyword]` to add a keyword to highlight in the current server. All of the text after `add` will be treated as one keyword.

Keywords are case-insensitive.

You're only notified of keywords when they appear in channels you follow. You can follow a channel with `@{name} follow [channel]`; see `@{name} help follow` for more information.

You can remove keywords later with `@{name} remove [keyword]`.

You can list your current keywords with `@{name} keywords`.",
					name = username
				)
		},
		CommandInfo {
			name: "follow",
			short_desc: "Follow a channel to be notified when your keywords appear there",
			long_desc: format!(
"Use `@{name} follow [channels]` to follow the specified channel and be notified when your keywords appear there. `[channels]` may be channel mentions, channel names, or channel IDs. You can specify multiple channels, separated by spaces, to follow all of them at once.

You're only notified of your keywords in channels you follow. You can add a keyword with `@{name} add [keyword]`; see `@{name} help add` for more information.

You can unfollow channels later with `@{name} unfollow [channels]`.

You can list your current followed channels with `@{name} follows`.",
				name = username,
			)
		},
		CommandInfo {
			name: "remove",
			short_desc: "Remove a keyword to highlight in the current server",
			long_desc: format!(
"Use `@{name} remove [keyword]` to remove a keyword that you previously added with `@{name} add` in the current server. All of the text after `remove` will be treated as one keyword.

Keywords are case-insensitive.

You can list your current keywords with `@{name} keywords`.",
				name = username,
			)
		},
		CommandInfo {
			name: "unfollow",
			short_desc: "Unfollow a channel, stopping notifications about your keywords appearing there",
			long_desc: format!(
"Use `@{name} unfollow [channels]` to unfollow channels and stop notifications about your keywords appearing there. `[channels]` may be channel mentions, channel names, or channel IDs. You can specify multiple channels, separated by spaces, to follow all of them at once.

You can list your current followed channels with `@{name} follows`.",
				name = username,
			)
		},
		CommandInfo {
			name: "help",
			short_desc: "Show this help message",
			long_desc: format!(
"Use `@{name} help` to see a list of commands and short descriptions.
Use `@{name} help [command]` to see additional information about the specified command.
Use `@{name} about` to see information about this bot.",
				name = username
			),
		},
		CommandInfo {
			name: "about",
			short_desc: "Show some information about this bot",
			long_desc: "Show some information about this bot, like version and source code.".to_owned(),
		},
	];

	if args == "" {
		message
			.channel_id
			.send_message(&ctx, |m| {
				m.embed(|e| 
					e.title(format!("{} – Help", username))
						.description(format!("Use `@{} help [command]` to see more information about a specified command", username))
						.fields(commands.iter().map(|info| (info.name, info.short_desc, true)))
						.color(EMBED_COLOR)
				)
			})
			.await?;
	} else {
		let info = match commands.iter().find(|info| info.name.eq_ignore_ascii_case(args)) {
			Some(info) => info,
			None => return question(ctx, &message).await,
		};

		message.channel_id.send_message(&ctx, |m| {
			m.embed(|e| {
				e.title(format!("Help – {}", info.name))
					.description(&info.long_desc)
					.color(EMBED_COLOR)	
			})
		}).await?;
	}

	Ok(())
}

pub async fn about(
	ctx: &Context,
	message: &Message,
	_: &str,
) -> Result<(), Error> {
	let invite_url = ctx
		.cache
		.current_user()
		.await
		.invite_url(&ctx, Permissions::empty())
		.await?;
	message
		.channel_id
		.send_message(ctx, |m| {
			m.embed(|e| {
				e.title(concat!(
					env!("CARGO_PKG_NAME"),
					" ",
					env!("CARGO_PKG_VERSION")
				))
				.field("Source", env!("CARGO_PKG_REPOSITORY"), true)
				.field("Author", "ThatsNoMoon#0175", true)
				.field(
					"Invite",
					format!("[Add me to your server]({})", invite_url),
					true,
				)
				.color(EMBED_COLOR)
			})
		})
		.await?;

	Ok(())
}
