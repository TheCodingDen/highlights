// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

#[macro_use]
mod util;

mod keywords;
pub use keywords::{
	add, ignore, ignores, keywords, remove, remove_server, unignore,
};

mod mutes;
pub use mutes::{mute, mutes, unmute};

mod blocks;
pub use blocks::{block, blocks, unblock};

use indoc::formatdoc;
use serenity::{
	client::Context,
	model::{channel::Message, Permissions},
};

use std::time::Instant;

use crate::{
	global::{settings, EMBED_COLOR},
	monitoring::{avg_command_time, avg_query_time, Timer},
	util::question,
	Error,
};

pub async fn ping(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("ping");
	require_empty_args!(args, ctx, message);
	let start = Instant::now();
	let mut sent_message = message.channel_id.say(ctx, "Ping... 🏓").await?;
	let seconds = start.elapsed().as_secs_f64();

	let message_latency = format_seconds(seconds);

	let cmd_latency = avg_command_time()
		.map(format_seconds)
		.unwrap_or_else(|| "<None>".to_owned());

	let db_latency = avg_query_time()
		.map(format_seconds)
		.unwrap_or_else(|| "<None>".to_owned());

	let reply = formatdoc!(
		"
		🏓 Pong!

		API Latency: {}
		Average Recent Command Latency: {}
		Average Recent Database Latency: {}
		",
		message_latency,
		cmd_latency,
		db_latency,
	);

	sent_message.edit(&ctx, |m| m.content(reply)).await?;

	Ok(())
}

fn format_seconds(seconds: f64) -> String {
	if seconds >= 10.0 {
		format!("{:.2} s", seconds)
	} else if seconds >= 0.0001 {
		format!("{:.2} ms", seconds * 1000.0)
	} else {
		// I would love for this to ever happen
		format!("{:.2} μs", seconds * 1_000_000.0)
	}
}

pub async fn about(
	ctx: &Context,
	message: &Message,
	args: &str,
) -> Result<(), Error> {
	let _timer = Timer::command("about");
	require_empty_args!(args, ctx, message);
	let invite_url = if settings().bot.private {
		None
	} else {
		Some(
			ctx.cache
				.current_user()
				.await
				.invite_url(&ctx, Permissions::empty())
				.await?,
		)
	};
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
				.color(EMBED_COLOR);

				if let Some(invite_url) = invite_url {
					e.field(
						"Invite",
						format!("[Add me to your server]({})", invite_url),
						true,
					);
				};
				e
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
		examples: Option<String>,
	}

	let username = ctx.cache.current_user_field(|u| u.name.clone()).await;

	let commands = [
		CommandInfo {
			name: "add",
			short_desc: "Add a keyword to highlight in the current server",
			long_desc: formatdoc!("
				Use `@{name} add [keyword]` to add a keyword to highlight in the current server. \
				{name} will notify you (in DMs) about any messages containing your keywords \
				(other than messages in muted channels or messages with ignored phrases).

				In this usage, all of the text after `add` will be treated as one keyword.

				Keywords are case-insensitive.

				You can also add a keyword in just a specific channel or channels with \
				`@{name} add \"[keyword]\" in [channels]`. \
				You'll only be notified of keywords added this way when they appear in the \
				specified channel(s) (not when they appear anywhere else). \
				The keyword must be surrounded with quotes, and you can use `\\\"` to add a \
				keyword with a quote in it. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to add the keyword in \
				all of them at once.

				You can remove keywords later with `@{name} remove [keyword]`; see \
				`@{name} help remove` for more information.

				You can list your current keywords with `@{name} keywords`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Add the keyword \"rust\" in the current server:
				`@{name} add rust`

				Add the keyword \"optimize\" in only the #javascript channel:
				`@{name} add \"optimize\" in javascript`

				Add the keyword \"hello world\" in the current server:
				`@{name} add hello world`",
				name = username
			)),
		},
		CommandInfo {
			name: "remove",
			short_desc: "Remove a keyword to highlight in the current server",
			long_desc: formatdoc!("
				Use `@{name} remove [keyword]` to remove a keyword that you previously added \
				with `@{name} add` in the current server.

				In this usage, all of the text after `remove` will be treated as one keyword.

				Keywords are case-insensitive.

				You can also remove a keyword that you added to a specific channel or channels \
				with `@{name} remove \"[keyword]\" from [channels]`. \
				The keyword must be surrounded with quotes, and you can use `\\\"` to remove a \
				keyword with a quote in it. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to remove the keyword \
				from all of them at once.

				You can list your current keywords with `@{name} keywords`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Remove the keyword \"node\" from the current server:
				`@{name} remove node`

				Remove the keyword \"go\" from the #general channel:
				`@{name} remove \"go\" from general`",
				name = username,
			)),
		},
		CommandInfo {
			name: "mute",
			short_desc: "Mute a channel to prevent server keywords from being highlighted there",
			long_desc: formatdoc!("
				Use `@{name} mute [channels]` to mute the specified channel(s) and \
				prevent notifications about your server-wide keywords appearing there. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to mute all of them \
				at once.

				You'll still be notified about any channel-specific keywords you add to muted \
				channels. \
				See `@{name} help add` for more information about channel-specific keywords.

				You can unmute channels later with `@{name} unmute [channels]`.

				You can list your currently muted channels with `@{name} mutes`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Mute the #memes channel:
				`@{name} mute memes`

				Mute the #general channel, and the off-topic channel, and the channel with an ID of 73413749283:
				`@{name} mute #general off-topic 73413749283`",
				name = username
			)),
		},
		CommandInfo {
			name: "unmute",
			short_desc:
				"Unmute a channel, enabling notifications about server keywords appearing there",
			long_desc: formatdoc!("
				Use `@{name} unmute [channels]` to unmute channels you previously muted and \
				re-enable notifications about your keywords appearing there. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to unmute all of them at \
				once.

				You can list your currently muted channels with `@{name} mutes`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Unmute the #rust channel:
				`@{name} unmute rust`

				Unmute the #functional channel, and the elixir channel, and the channel with an ID of 73413749283:
				`@{name} unmute #functional elixir 73413749283`",
				name = username
			)),
		},
		CommandInfo {
			name: "block",
			short_desc: "Block a user to prevent your keywords in their messages from being highlighted",
			long_desc: formatdoc!("
				Use `@{name} block [users]` to block the specified users(s) and \
				prevent notifications about your keywords in their messages. \
				`[users]` may be user mentions or user IDs. \
				You can specify multiple users, separated by spaces, to block all of them \
				at once.

				You can unblock users later with `@{name} unblock [users]`.

				You can list your currently blocked users with `@{name} blocks`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Block AnnoyingUser:
				`@{name} block @AnnoyingUser`

				Block RidiculousPerson and the user with ID 669274872716
				`@{name} mute @RidiculousPerson 669274872716`",
				name = username
			)),
		},
		CommandInfo {
			name: "unblock",
			short_desc:
				"Unblock a user, enabling notifications about your keywords in their messages",
			long_desc: formatdoc!("
				Use `@{name} unblock [users]` to unblock users you previously blocked and \
				re-enable notifications about your keywords appearing in their messages. \
				`[users]` may be user mentions or user IDs. You can specify multiple users, \
				separated by spaces, to unblock all of them at once.

				You can list your currently blocked users with `@{name} blocks`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Unblock the user RedemptionArc:
				`@{name} unblock @RedemptionArc`

				Unmute the user AccidentallyTrollish and the user with an ID of 669274872716:
				`@{name} unblock @AccidentallyTrollish 669274872716`",
				name = username
			)),
		},
		CommandInfo {
			name: "ignore",
			short_desc: "Add a phrase to ignore in the current server",
			long_desc: formatdoc!("
				Use `@{name} ignore [phrase]` to add a phrase to ignore in the current server.

				You won't be notified of any messages that contain ignored phrases, even if they \
				contain one of your keywords.

				Phrases are case-insensitive.

				You can remove ignored phrases later with `@{name} unignore [phrase]`; see \
				`@{name} help unignore` for more information.

				You can list your current keywords with `@{name} ignores`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Ignore messages containing \"meme\" in the current server:
				`@{name} ignore meme`

				Ignore messages containing \"hello world\" in the current server:
				`@{name} ignore hello world`",
				name = username
			)),
		},
		CommandInfo {
			name: "unignore",
			short_desc: "Remove an ignored phrase in the current server",
			long_desc: formatdoc!("
				Use `@{name} ignore [phrase]` to remove a phrase you previously ignored in the \
				current server.

				Phrases are case-insensitive.

				You can list your current keywords with `@{name} ignores`.",
				name = username,
			),
			examples: Some(formatdoc!("
				Stop ignoring messages containing \"haskell\" in the current server:
				`@{name} unignore haskell`

				Stop ignoring messages containing \"map-reduce\" in the current server:
				`@{name} ignore map-reduce`",
				name = username
			)),
		},
		CommandInfo {
			name: "keywords",
			short_desc: "List your current highlighted keywords",
			long_desc: formatdoc!("
				Use `@{name} keywords` to list your current highlighted keywords.

				Using `keywords` in a server will show you only the keywords you've highlighted \
				in that server, including all channel-specific keywords there.

				Using `keywords` in DMs with the bot will list keywords you've highlighted \
				across all shared servers, including potentially deleted servers or servers this \
				bot is no longer a member of.

				If the bot can't find information about a server you have keywords in, \
				its ID will be in parentheses, so you can remove them with `remove-server` \
				if desired. \
				See `@{name} help remove-server` for more details.",
				name = username
			),
			examples: Some(formatdoc!("
				Display your current keywords:
				`@{name} keywords`",
				name = username
			)),
		},
		CommandInfo {
			name: "mutes",
			short_desc: "List your currently muted channels",
			long_desc: formatdoc!("
				Use `@{name} mutes` to list your currently muted channels.

				Using `mutes` in a server will only show you the channels you've muted in that \
				server.

				Using `mutes` in DMs with the bot will list channels you've muted across \
				all servers, including deleted channels or channels in servers this bot is \
				no longer a member of. If the bot can't find information on a channel you \
				previously muted, its ID will be in parentheses.",
				name = username
			),
			examples: Some(formatdoc!("
				Display your currently muted channels:
				`@{name} mutes`",
				name = username
			)),
		},
		CommandInfo {
			name: "blocks",
			short_desc: "List your currently blocked users",
			long_desc: formatdoc!("
				Use `@{name} blocks` to list your currently blocked users.",
				name = username
			),
			examples: Some(formatdoc!("
				Display your currently blocked users:
				`@{name} blocks`",
				name = username
			)),
		},
		CommandInfo {
			name: "ignores",
			short_desc: "List your currently ignored phrases",
			long_desc: formatdoc!("
				Use `@{name} ignores` to list your currently ignored phrases.

				Using `ignores` in a server will only show you the phrases you've ignored in that \
				server.

				Using `ignores` in DMs with the bot will list phrases you've ignored across \
				all servers, including servers this bot is no longer a member of.

				If the bot can't find information on a server you ignored phrases in, its ID will \
				be in parentheses, so you can use `remove-server` to remove the ignores there if \
				desired.",
				name = username
			),
			examples: Some(formatdoc!("
				Display your currently ignored phrases:
				`@{name} ignores`",
				name = username
			)),
		},
		CommandInfo {
			name: "remove-server",
			short_desc: "Remove all keywords and ignores on a given server",
			long_desc: formatdoc!("
				Use `@{name} remove-server [server ID]` to remove all keywords **and** ignores on \
				the server with the given ID.

				This won't remove channel-specific keywords in the given server; you can use the \
				normal `remove` command for that.

				This is normally not necessary, but if you no longer share a server with the bot \
				where you added keywords, you can clean up your keywords list by using `keywords` \
				in DMs to see all keywords, and this command to remove any server IDs the bot \
				can't find. ",
				name = username
			),
			examples: Some(formatdoc!("
				Remove all server-wide keywords and ignores added to the server with an ID of \
				126029834632:
				`@{name} remove-server 126029834632`",
				name = username
			)),
		},
		CommandInfo {
			name: "help",
			short_desc: "Show this help message",
			long_desc: formatdoc!("
				Use `@{name} help` to see a list of commands and short descriptions.
				Use `@{name} help [command]` to see additional information about \
				the specified command.
				Use `@{name} about` to see information about this bot.",
				name = username
			),
			examples: Some(formatdoc!("
				Display the list of commands:
				`@{name} help`

				Display the help for the `add` command:
				`@{name} help add`",
				name = username
			)),
		},
		CommandInfo {
			name: "ping",
			short_desc: "Show the bot's ping",
			long_desc: "Show the bot's ping, including current API, command, and database latency.".to_owned(),
			examples: None,
		},
		CommandInfo {
			name: "about",
			short_desc: "Show some information about this bot",
			long_desc:
				"Show some information about this bot, like version and source code.".to_owned(),
			examples: None,
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
						.color(EMBED_COLOR);

					match info.examples.as_ref() {
						Some(ex) => e.field("Example Usage", ex, false),
						None => e,
					}
				})
			})
			.await?;
	}

	Ok(())
}
