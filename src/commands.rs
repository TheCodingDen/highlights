// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::Lazy;
use serenity::{
	client::Context,
	model::{
		channel::{ChannelType, GuildChannel, Message},
		id::{ChannelId, GuildId},
		Permissions,
	},
};

use std::{collections::HashMap, convert::TryInto, fmt::Write};

use crate::{
	db::{Follow, Keyword},
	global::{EMBED_COLOR, MAX_KEYWORDS},
	monitoring::Timer,
	util::{error, question, success},
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
	let _timer = Timer::command("add");
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
		guild_id: guild_id.0.try_into().unwrap(),
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

	success(ctx, message).await
}

pub async fn remove(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("remove");
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	let keyword = Keyword {
		keyword: args.to_lowercase(),
		user_id: message.author.id.0.try_into().unwrap(),
		guild_id: guild_id.0.try_into().unwrap(),
	};

	if !keyword.clone().exists().await? {
		return error(ctx, message, "You haven't added that keyword!").await;
	}

	keyword.delete().await?;

	success(ctx, message).await
}

pub async fn remove_server(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("removeserver");
	check_empty_args!(args, ctx, message);

	let guild_id = match args.parse() {
		Ok(id) => GuildId(id),
		Err(_) => return error(ctx, message, "Invalid server ID!").await,
	};

	match Keyword::delete_in_guild(message.author.id, guild_id).await? {
		0 => {
			error(
				ctx,
				message,
				"You didn't have any keywords with that server ID!",
			)
			.await
		}
		_ => success(ctx, message).await,
	}
}

pub async fn follow(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("follow");
	let guild_id = check_guild!(ctx, message);

	check_empty_args!(args, ctx, message);

	let user_id = message.author.id;
	let self_id = ctx.cache.current_user_id().await;

	let channels = ctx.cache.guild_channels(guild_id).await.unwrap();
	let channels = channels
		.into_iter()
		.filter(|(_, channel)| channel.kind == ChannelType::Text)
		.collect();

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

	message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	Ok(())
}

pub async fn unfollow(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("unfollow");
	check_empty_args!(args, ctx, message);

	let channels = match message.guild_id {
		Some(guild_id) => {
			ctx.cache.guild_channels(guild_id).await.map(|channels| {
				channels
					.into_iter()
					.filter(|(_, channel)| channel.kind == ChannelType::Text)
					.collect()
			})
		}
		None => None,
	};

	let user_id = message.author.id.0.try_into().unwrap();

	let mut unfollowed = vec![];
	let mut not_followed = vec![];
	let mut not_found = vec![];

	for arg in args.split_whitespace() {
		let channel = channels
			.as_ref()
			.and_then(|channels| get_channel_from_arg(channels, arg));

		match channel {
			None => {
				if let Ok(channel_id) = arg.parse::<u64>() {
					let channel_id = channel_id.try_into().unwrap();

					let follow = Follow {
						user_id,
						channel_id,
					};

					if !follow.clone().exists().await? {
						not_found.push(arg);
					} else {
						unfollowed.push(format!("<#{0}> ({0})", channel_id));
						follow.delete().await?;
					}
				} else {
					not_found.push(arg);
				}
			}
			Some(channel) => {
				let channel_id = channel.id.0.try_into().unwrap();

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

	message
		.channel_id
		.send_message(ctx, |m| {
			m.content(msg).allowed_mentions(|m| m.empty_parse())
		})
		.await?;

	Ok(())
}

fn get_channel_from_arg<'c>(
	channels: &'c HashMap<ChannelId, GuildChannel>,
	arg: &str,
) -> Option<&'c GuildChannel> {
	if let Ok(id) = arg.parse::<u64>() {
		return channels.get(&ChannelId(id));
	}

	if let Some(id) = arg
		.strip_prefix("<#")
		.and_then(|arg| arg.strip_suffix(">"))
		.and_then(|arg| arg.parse::<u64>().ok())
	{
		return channels.get(&ChannelId(id));
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

pub async fn keywords(
	ctx: &Context,
	message: &Message,
	_: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("keywords");
	match message.guild_id {
		Some(guild_id) => {
			let keywords =
				Keyword::user_keywords_in_guild(message.author.id, guild_id)
					.await?
					.into_iter()
					.map(|keyword| keyword.keyword)
					.collect::<Vec<_>>();

			if keywords.is_empty() {
				return error(
					ctx,
					message,
					"You haven't added any keywords yet!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.ok_or("Couldn't get guild to list keywords")?;

			let response = format!(
				"{}'s keywords in {}:\n  - {}",
				message.author.name,
				guild_name,
				keywords.join("\n  - ")
			);

			message
				.channel_id
				.send_message(ctx, |m| {
					m.content(response).allowed_mentions(|m| m.empty_parse())
				})
				.await?;
		}
		None => {
			let keywords = Keyword::user_keywords(message.author.id).await?;

			if keywords.is_empty() {
				return error(
					ctx,
					message,
					"You haven't added any keywords yet!",
				)
				.await;
			}

			let mut keywords_by_guild = HashMap::new();

			for keyword in keywords {
				let guild_id = GuildId(keyword.guild_id.try_into().unwrap());

				keywords_by_guild
					.entry(guild_id)
					.or_insert_with(Vec::new)
					.push(keyword.keyword);
			}

			let mut response = String::new();

			for (guild_id, keywords) in keywords_by_guild {
				if !response.is_empty() {
					response.push_str("\n\n");
				}

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.await
					.unwrap_or_else(|| {
						format!("<Unknown server> ({})", guild_id)
					});

				write!(
					&mut response,
					"Your keywords in {}:\n  – {}",
					guild_name,
					keywords.join("\n  – ")
				)
				.unwrap();
			}

			message.channel_id.say(ctx, response).await?;
		}
	}

	Ok(())
}

pub async fn follows(
	ctx: &Context,
	message: &Message,
	_: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("follows");
	match message.guild_id {
		Some(guild_id) => {
			let channels = ctx
				.cache
				.guild_channels(guild_id)
				.await
				.ok_or("Couldn't get guild channels to list follows")?;

			let follows = Follow::user_follows(message.author.id)
				.await?
				.into_iter()
				.filter(|follow| {
					let channel_id =
						ChannelId(follow.channel_id.try_into().unwrap());
					channels.contains_key(&channel_id)
				})
				.map(|follow| format!("<#{}>", follow.channel_id))
				.collect::<Vec<_>>();

			if follows.is_empty() {
				return error(
					ctx,
					message,
					"You haven't followed any channels yet!",
				)
				.await;
			}

			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.ok_or("Couldn't get guild to list follows")?;

			let response = format!(
				"{}'s follows in {}:\n  - {}",
				message.author.name,
				guild_name,
				follows.join("\n  - ")
			);

			message.channel_id.say(ctx, response).await?;
		}
		None => {
			let follows = Follow::user_follows(message.author.id).await?;

			if follows.is_empty() {
				return error(
					ctx,
					message,
					"You haven't followed any channels yet!",
				)
				.await;
			}

			let mut follows_by_guild = HashMap::new();
			let mut not_found = Vec::new();

			for follow in follows {
				let channel_id =
					ChannelId(follow.channel_id.try_into().unwrap());
				let channel = match ctx.cache.guild_channel(channel_id).await {
					Some(channel) => channel,
					None => {
						not_found.push(format!("<#{0}> ({0})", channel_id));
						continue;
					}
				};

				follows_by_guild
					.entry(channel.guild_id)
					.or_insert_with(Vec::new)
					.push(format!("<#{}>", channel_id));
			}

			let mut response = String::new();

			for (guild_id, channel_ids) in follows_by_guild {
				if !response.is_empty() {
					response.push_str("\n\n");
				}

				let guild_name = ctx
					.cache
					.guild_field(guild_id, |g| g.name.clone())
					.await
					.ok_or("Couldn't get guild to list follows")?;

				write!(
					&mut response,
					"Your follows in {}:\n  – {}",
					guild_name,
					channel_ids.join("\n  – ")
				)
				.unwrap();
			}

			if !not_found.is_empty() {
				write!(
					&mut response,
					"\n\nCouldn't find (deleted?) followed channels:\n  – {}",
					not_found.join("\n  – ")
				)
				.unwrap();
			}

			message.channel_id.say(ctx, response).await?;
		}
	}

	Ok(())
}

pub async fn about(
	ctx: &Context,
	message: &Message,
	_: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("about");
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

pub async fn help(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("help");
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
				"Use `@{name} add [keyword]` to add a keyword to highlight in the current server. \
				All of the text after `add` will be treated as one keyword.

				Keywords are case-insensitive.

				You're only notified of keywords when they appear in channels you follow. \
				You can follow a channel with `@{name} follow [channel]`; \
				see `@{name} help follow` for more information.

				You can remove keywords later with `@{name} remove [keyword]`.

				You can list your current keywords with `@{name} keywords`.",
				name = username,
			)
		},
		CommandInfo {
			name: "follow",
			short_desc: "Follow a channel to be notified when your keywords appear there",
			long_desc: format!(
				"Use `@{name} follow [channels]` to follow the specified channel and \
				be notified when your keywords appear there. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, \
				to follow all of them at once.

				You're only notified of your keywords in channels you follow. \
				You can add a keyword with `@{name} add [keyword]`; \
				see `@{name} help add` for more information.

				You can unfollow channels later with `@{name} unfollow [channels]`.

				You can list your current followed channels with `@{name} follows`.",
				name = username,
			)
		},
		CommandInfo {
			name: "remove",
			short_desc: "Remove a keyword to highlight in the current server",
			long_desc: format!(
				"Use `@{name} remove [keyword]` to remove a keyword that you \
				previously added with `@{name} add` in the current server. \
				All of the text after `remove` will be treated as one keyword.

				Keywords are case-insensitive.

				You can list your current keywords with `@{name} keywords`.",
				name = username,
			)
		},
		CommandInfo {
			name: "unfollow",
			short_desc:
				"Unfollow a channel, stopping notifications about your keywords appearing there",
			long_desc: format!(
				"Use `@{name} unfollow [channels]` to unfollow channels and stop notifications \
				about your keywords appearing there. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to follow all of them \
				at once.

				You can list your current followed channels with `@{name} follows`.",
				name = username,
			)
		},
		CommandInfo {
			name: "keywords",
			short_desc: "List your current highlighted keywords",
			long_desc: format!(
				"Use `@{name} keywords` to list your current highlighted keywords.

				Using `keywords` in a server will show you only the keywords you've highlighted \
				in that server.

				Using `keywords` in DMs with the bot will list keywords you've highlighted \
				across all shared servers, including potentially deleted servers or servers this \
				bot is no longer a member of.

				If the bot can't find information about a server you have keywords in, \
				its ID will be in parentheses, so you can remove them with `removeserver` \
				if desired. See `@{name} help removeserver` for more details.",
				name = username
			)
		},
		CommandInfo {
			name: "follows",
			short_desc: "List your current followed channels",
			long_desc: format!(
				"Use `@{name} follows` to list your current followed channels.

				Using `follows` in a server will show you only the channels you've followed \
				in that server.

				Using `follows` in DMs with the bot will list channels you've followed across \
				all servers, including deleted channels or channels in servers this bot is \
				no longer a member of.

				If the bot can't find information on a channel you previously followed, \
				its ID will be in parentheses, so you can investigate or unfollow.",
				name = username
			)
		},
		CommandInfo {
			name: "removeserver",
			short_desc: "Remove all keywords on a given server",
			long_desc: format!(
				"Use `@{name} removeserver [server ID]` to remove all keywords on the server \
				with the given ID.

				This is normally not necessary, but if you no longer share a server with the bot \
				where you added keywords, you can clean up your keywords list by using `keywords` \
				in DMs to see all keywords, and this command to remove any server IDs the bot \
				can't find.",
				name = username
			)
		},
		CommandInfo {
			name: "help",
			short_desc: "Show this help message",
			long_desc: format!(
				"Use `@{name} help` to see a list of commands and short descriptions.
				Use `@{name} help [command]` to see additional information about \
				the specified command.
				Use `@{name} about` to see information about this bot.",
				name = username
			),
		},
		CommandInfo {
			name: "about",
			short_desc: "Show some information about this bot",
			long_desc:
				"Show some information about this bot, like version and source code.".to_owned(),
		},
	];

	if args == "" {
		message
			.channel_id
			.send_message(&ctx, |m| {
				m.embed(|e| {
					e.title(format!("{} – Help", username))
						.description(format!(
							"Use `@{} help [command]` to see more information \
							about a specified command",
							username
						))
						.fields(
							commands
								.iter()
								.map(|info| (info.name, info.short_desc, true)),
						)
						.color(EMBED_COLOR)
				})
			})
			.await?;
	} else {
		let info = match commands
			.iter()
			.find(|info| info.name.eq_ignore_ascii_case(args))
		{
			Some(info) => info,
			None => return question(ctx, &message).await,
		};

		message
			.channel_id
			.send_message(&ctx, |m| {
				m.embed(|e| {
					e.title(format!("Help – {}", info.name))
						.description(&info.long_desc)
						.color(EMBED_COLOR)
				})
			})
			.await?;
	}

	Ok(())
}
