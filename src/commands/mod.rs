// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

#[macro_use]
mod util;

mod keywords;
pub use keywords::{add, keywords, remove, remove_server};

mod mutes;
pub use mutes::{mute, mutes, unmute};

use serenity::{
	client::Context,
	model::{channel::Message, Permissions},
};

use crate::{global::EMBED_COLOR, monitoring::Timer, util::question, Error};

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
			)
		},
		CommandInfo {
			name: "remove",
			short_desc: "Remove a keyword to highlight in the current server",
			long_desc: format!(
				"Use `@{name} remove [keyword]` to remove a keyword that you previously added \
				with `@{name} add` in the current server. \
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
			)
		},
		CommandInfo {
			name: "mute",
			short_desc: "Mute a channel to prevent server keywords from being highlighted there",
			long_desc: format!(
				"Use `@{name} mute [channels]` to mute the specified channel(s) and \
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
			)
		},
		CommandInfo {
			name: "unmute",
			short_desc:
				"Unmite a channel, enabling notifications about server keywords appearing there",
			long_desc: format!(
				"Use `@{name} unmute [channels]` to unmute channels you previously muted and \
				re-enable notifications about your keywords appearing there. \
				`[channels]` may be channel mentions, channel names, or channel IDs. \
				You can specify multiple channels, separated by spaces, to unmute all of them at \
				once.

				You can list your currently muted channels with `@{name} mutes`.",
				name = username,
			)
		},
		CommandInfo {
			name: "keywords",
			short_desc: "List your current highlighted keywords",
			long_desc: format!(
				"Use `@{name} keywords` to list your current highlighted keywords.

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
			)
		},
		CommandInfo {
			name: "mutes",
			short_desc: "List your currently muted channels",
			long_desc: format!(
				"Use `@{name} mutes` to list your currently muted channels.

				Using `mutes` in a server will show you only the channels you've muted in that \
				server.

				Using `mutes` in DMs with the bot will list channels you've muted across \
				all servers, including deleted channels or channels in servers this bot is \
				no longer a member of.

				If the bot can't find information on a channel you previously followed, \
				its ID will be in parentheses, so you can investigate or unmute.",
				name = username
			)
		},
		CommandInfo {
			name: "remove-server",
			short_desc: "Remove all server-wide keywords on a given server",
			long_desc: format!(
				"Use `@{name} remove-server [server ID]` to remove all keywords on the server \
				with the given ID.

				This won't remove channel-specific keywords in the given server; you can use the \
				normal `remove` command for that.

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