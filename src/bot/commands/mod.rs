// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Implementations of all of the explicit bot commands.

#[macro_use]
mod util;
mod blocks;
mod keywords;
mod mutes;
mod opt_out;

use std::fmt::Write;

use anyhow::{Context as _, Result};
use indoc::indoc;
use once_cell::sync::Lazy;
use serenity::{
	builder::{CreateApplicationCommand, CreateApplicationCommandOption},
	client::Context,
	model::{
		application::{
			command::Command as ApplicationCommand,
			interaction::{
				application_command::ApplicationCommandInteraction as Command,
				MessageFlags,
			},
			oauth::Scope,
		},
		Permissions,
	},
};
use tracing::{debug, info};

pub(crate) use self::{
	blocks::{block, blocks, unblock},
	keywords::{
		add, ignore, ignores, keywords, remove, remove_server, unignore,
	},
	mutes::{mute, mutes, unmute},
	opt_out::{opt_in, opt_out},
};
use super::Shards;
use crate::{
	bot::{util::respond, STARTED},
	global::EMBED_COLOR,
	require_embed_perms,
	settings::settings,
};

// Create all slash commands globally, and in a test guild if configured.
pub(crate) async fn create_commands(ctx: Context) {
	info!("Registering slash commands");
	let commands = COMMAND_INFO
		.iter()
		.map(CommandInfo::create)
		.collect::<Vec<_>>();
	if let Some(guild) = settings().bot.test_guild {
		debug!("Registering commands in test guild");

		guild
			.set_application_commands(&ctx, |create| {
				create.set_application_commands(commands.clone())
			})
			.await
			.expect("Failed to create guild application commands");
	}
	ApplicationCommand::set_global_application_commands(&ctx, |create| {
		create.set_application_commands(commands)
	})
	.await
	.expect("Failed to set global application commands");
}

/// Display the API latency of the bot.
///
/// Usage: `/ping`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn ping(ctx: Context, command: Command) -> Result<()> {
	let latency = ctx
		.data
		.read()
		.await
		.get::<Shards>()
		.expect("Shard manager not stored in client data")
		.lock()
		.await
		.runners
		.lock()
		.await
		.values()
		.next()
		.expect("No shards managed")
		.latency;

	let mut reply = "🏓 Pong!".to_owned();

	if let Some(latency) = latency {
		reply += "\nGateway latency: ";

		let micros = latency.as_micros();
		if micros > 10_000_000 {
			write!(&mut reply, "{:.2} s", micros as f64 / 1_000_000.0).unwrap();
		} else if micros > 10 {
			write!(&mut reply, "{:.2} ms", micros as f64 / 1000.0).unwrap();
		} else {
			write!(&mut reply, "{} μs", micros).unwrap();
		}
	}

	respond(&ctx, &command, &reply).await?;

	Ok(())
}

/// Displays information about the bot.
///
/// Usage: `/about`
///
/// Displays the cargo package name and version, cargo source, author, and an
/// invite URL.
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn about(ctx: Context, command: Command) -> Result<()> {
	require_embed_perms!(&ctx, &command);

	let invite_url = if settings().bot.private {
		None
	} else {
		let scopes = [Scope::Bot, Scope::ApplicationsCommands];
		Some(
			ctx.cache
				.current_user()
				.invite_url_with_oauth2_scopes(
					&ctx,
					Permissions::empty(),
					&scopes,
				)
				.await?,
		)
	};

	let uptime = STARTED.get().expect("Start time not set").elapsed();

	let uptime = {
		let seconds = uptime.as_secs();

		let days = seconds / 86_400;
		let hours = seconds / 3600 % 24;
		let minutes = seconds / 60 % 60;
		let seconds = seconds % 60;

		if days >= 20 {
			format!("{days} days")
		} else if days > 0 {
			format!("{days} days, {hours} hours")
		} else if hours > 0 {
			format!("{hours} hours, {minutes} minutes")
		} else if minutes > 0 {
			format!("{minutes} minutes, {seconds} seconds")
		} else {
			format!("{seconds} seconds")
		}
	};

	command
		.create_interaction_response(&ctx, |r| {
			r.interaction_response_data(|m| {
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
					e.field("Uptime", uptime, true)
				})
			})
		})
		.await
		.context("Failed to send about message")?;

	Ok(())
}

/// Displays information about using the bot.
///
/// Usage: `/help [command]`
///
/// When given no arguments, displays the list of commands. When given an
/// argument, displays detailed information about the command of that name.
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn help(ctx: Context, command: Command) -> Result<()> {
	require_embed_perms!(&ctx, &command);

	let username = ctx.cache.current_user_field(|u| u.name.clone());

	match command.data.options.get(0) {
		None => {
			command
				.create_interaction_response(&ctx, |r| {
					r.interaction_response_data(|m| {
						m.flags(MessageFlags::EPHEMERAL).embed(|e| {
							e.title(format!("{} – Help", username))
								.description(
									"Use `/help [command]` to see more \
									information about a specified command",
								)
								.fields(COMMAND_INFO.iter().map(|info| {
									(info.name, info.short_desc, true)
								}))
								.color(EMBED_COLOR)
						})
					})
				})
				.await?
		}
		Some(option) => {
			let name = option
				.value
				.as_ref()
				.context("Command option has no value")?
				.as_str()
				.context("Command option is not string")?;
			let info = match COMMAND_INFO
				.iter()
				.find(|info| info.name.eq_ignore_ascii_case(name))
			{
				Some(info) => info,
				None => {
					return Err(anyhow::anyhow!(
						"Invalid command name passed to help: {}",
						name
					));
				}
			};

			command
				.create_interaction_response(&ctx, |r| {
					r.interaction_response_data(|m| {
						m.flags(MessageFlags::EPHEMERAL).embed(|e| {
							e.title(format!("Help – {}", info.name))
								.description(&info.long_desc)
								.color(EMBED_COLOR);

							match info.examples.as_ref() {
								Some(ex) => e.field("Example Usage", ex, false),
								None => e,
							}
						})
					})
				})
				.await?
		}
	}

	Ok(())
}

/// Description of a command for the help command and slash command creation.
struct CommandInfo {
	name: &'static str,
	short_desc: &'static str,
	long_desc: &'static str,
	examples: Option<&'static str>,
	options: Vec<CreateApplicationCommandOption>,
}

impl CommandInfo {
	/// Create a [`CreateApplicationCommand`] describing this command to create
	/// a corresponding slash command.
	fn create(&self) -> CreateApplicationCommand {
		let mut builder = CreateApplicationCommand::default();
		builder
			.name(self.name)
			.description(self.short_desc)
			.set_options(self.options.clone());
		builder
	}
}

static COMMAND_INFO: Lazy<[CommandInfo; 18], fn() -> [CommandInfo; 18]> =
	Lazy::new(|| {
		use serenity::{
			builder::CreateApplicationCommandOption as Option,
			model::application::command::CommandOptionType,
		};
		let mut commands = [
			CommandInfo {
				name: "add",
				short_desc:
					"Add a keyword to highlight in the current server or a specific channel",
				long_desc: indoc!("
					Use `/add [keyword]` to add a keyword to highlight in the current server. \
					You'll be notified (in DMs) about any messages containing your keywords \
					(other than messages in muted channels or messages with ignored phrases).

					Keywords are case-insensitive.

					You can also add a keyword in just a specific channel or channels with \
					`/add [keyword] [channel]`. \
					You'll only be notified of keywords added this way when they appear in the \
					specified channel(s) (not when they appear anywhere else).
					
					You can remove keywords later with `/remove [keyword]`; see \
					`/help remove` for more information.

					You can list your current keywords with `/keywords`.",
				),
				examples: Some(indoc!("
					Add the keyword \"rust\" in the current server:
					/add `keyword:` rust

					Add the keyword \"optimize\" in only the #javascript channel:
					/add `keyword:` optimize `channel:` javascript

					Add the keyword \"hello world\" in the current server:
					/add `keyword:` hello world",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("keyword")
							.description("The keyword to listen to")
							.kind(CommandOptionType::String)
							.required(true);
						opt
					},
					{
						let mut opt = Option::default();
						opt
							.name("channel")
							.description("A specific channel for this keyword")
							.kind(CommandOptionType::Channel);
						opt
					}
				],
			},
			CommandInfo {
				name: "remove",
				short_desc: "Remove a keyword to highlight in the current server",
				long_desc: indoc!("
					Use `/remove [keyword]` to remove a keyword that you previously added \
					with `/add` in the current server.

					Keywords are case-insensitive.

					You can also remove a keyword that you added to a specific channel or channels \
					with `/remove [keyword] [channel]`. \
					The keyword must be surrounded with quotes, and you can use `\\\"` to remove a \
					keyword with a quote in it.

					You can list your current keywords with `/keywords`.",
				),
				examples: Some(indoc!("
					Remove the keyword \"node\" from the current server:
					/remove `keyword:` node

					Remove the keyword \"go\" from the #general channel:
					/remove `keyword: `go `channel:` #general",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("keyword")
							.description("The keyword to listen to")
							.kind(CommandOptionType::String)
							.required(true);
						opt
					},
					{
						let mut opt = Option::default();
						opt
							.name("channel")
							.description("The specific channel for this keyword")
							.kind(CommandOptionType::Channel);
						opt
					}
				],
			},
			CommandInfo {
				name: "mute",
				short_desc: "Mute a channel to prevent server keywords from being highlighted there",
				long_desc: indoc!("
					Use `/mute [channel]` to mute the specified channel(s) and \
					prevent notifications about your server-wide keywords appearing there.

					You'll still be notified about any channel-specific keywords you add to muted \
					channels. \
					See `/help add` for more information about channel-specific keywords.

					You can unmute channels later with `/unmute [channels]`.

					You can list your currently muted channels with `/mutes`.",
				),
				examples: Some(indoc!("
					Mute the #memes channel:
					/mute `channel:` #memes",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("channel")
							.description("The channel to mute")
							.kind(CommandOptionType::Channel)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "unmute",
				short_desc:
					"Unmute a channel, enabling notifications about server keywords appearing there",
				long_desc: indoc!("
					Use `/unmute [channel]` to unmute channels you previously muted and \
					re-enable notifications about your keywords appearing there.

					You can list your currently muted channels with `/mutes`.",
				),
				examples: Some(indoc!("
					Unmute the #rust channel:
					/unmute `channel:` #rust",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("channel")
							.description("The channel to unmute")
							.kind(CommandOptionType::Channel)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "block",
				short_desc: "Block a user to prevent your keywords in their messages from being highlighted",
				long_desc: indoc!("
					Use `/block [user]` to block the specified users and \
					prevent notifications about your keywords in their messages.

					You can unblock users later with `/unblock [user]`.

					You can list your currently blocked users with `/blocks`.",
				),
				examples: Some(indoc!("
					Block AnnoyingUser:
					/block `user:` @AnnoyingUser",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("user")
							.description("The user to block")
							.kind(CommandOptionType::User)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "unblock",
				short_desc:
					"Unblock a user, enabling notifications about your keywords in their messages",
				long_desc: indoc!("
					Use `/unblock [user]` to unblock a user you previously blocked and \
					re-enable notifications about your keywords appearing in their messages.

					You can list your currently blocked users with `/blocks`.",
				),
				examples: Some(indoc!("
					Unblock the user RedemptionArc:
					/unblock `user:` @RedemptionArc",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("user")
							.description("The user to unblock")
							.kind(CommandOptionType::User)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "ignore",
				short_desc: "Add a phrase to ignore in the current server",
				long_desc: indoc!("
					Use `/ignore [phrase]` to add a phrase to ignore in the current server.

					You won't be notified of any messages that contain ignored phrases, even if they \
					contain one of your keywords.

					Phrases are case-insensitive.

					You can remove ignored phrases later with `/unignore [phrase]`; see \
					`/help unignore` for more information.

					You can list your current keywords with `/ignores`.",
				),
				examples: Some(indoc!("
					Ignore messages containing \"meme\" in the current server:
					/ignore `phrase:` meme

					Ignore messages containing \"hello world\" in the current server:
					/ignore `phrase:` hello world",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("phrase")
							.description("The phrase to ignore")
							.kind(CommandOptionType::String)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "unignore",
				short_desc: "Remove an ignored phrase in the current server",
				long_desc: indoc!("
					Use `/unignore [phrase]` to remove a phrase you previously ignored in the \
					current server.

					Phrases are case-insensitive.

					You can list your current keywords with `/ignores`.",
				),
				examples: Some(indoc!("
					Stop ignoring messages containing \"haskell\" in the current server:
					/unignore `phrase:` haskell

					Stop ignoring messages containing \"map-reduce\" in the current server:
					/ignore `phrase:` map-reduce",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("phrase")
							.description("The phrase to unignore")
							.kind(CommandOptionType::String)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "keywords",
				short_desc: "List your current highlighted keywords",
				long_desc: indoc!("
					Use `/keywords` to list your current highlighted keywords.

					Using `keywords` in a server will show you only the keywords you've highlighted \
					in that server, including all channel-specific keywords there.

					If the bot can't find information about a server you have keywords in, \
					its ID will be in parentheses, so you can remove them with `remove-server` \
					if desired. \
					See `/help remove-server` for more details.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "mutes",
				short_desc: "List your currently muted channels",
				long_desc: indoc!("
					Use `/mutes` to list your currently muted channels.

					Using `mutes` in a server will only show you the channels you've muted in that \
					server.

					Using `mutes` in DMs with the bot will list channels you've muted across \
					all servers, including deleted channels or channels in servers this bot is \
					no longer a member of. If the bot can't find information on a channel you \
					previously muted, its ID will be in parentheses.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "blocks",
				short_desc: "List your currently blocked users",
				long_desc: indoc!("
					Use `/blocks` to list your currently blocked users.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "ignores",
				short_desc: "List your currently ignored phrases",
				long_desc: indoc!("
					Use `/ignores` to list your currently ignored phrases.

					Using `ignores` in a server will only show you the phrases you've ignored in that \
					server.

					Using `ignores` in DMs with the bot will list phrases you've ignored across \
					all servers, including servers this bot is no longer a member of.

					If the bot can't find information on a server you ignored phrases in, its ID will \
					be in parentheses, so you can use `remove-server` to remove the ignores there if \
					desired.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "remove-server",
				short_desc: "Remove all keywords and ignores on a given server",
				long_desc: indoc!("
					Use `/remove-server [server ID]` to remove all keywords **and** ignores on \
					the server with the given ID.

					This won't remove channel-specific keywords in the given server; you can use the \
					normal `remove` command for that.

					This is normally not necessary, but if you no longer share a server with the bot \
					where you added keywords, you can clean up your keywords list by using `keywords` \
					in DMs to see all keywords, and this command to remove any server IDs the bot \
					can't find. ",
				),
				examples: Some(indoc!("
					Remove all server-wide keywords and ignores added to the server with an ID of \
					126029834632:
					/remove-server `server:` 126029834632",
				)),
				options: vec![
					{
						let mut opt = Option::default();
						opt
							.name("server")
							.description("The ID of the server to remove")
							.kind(CommandOptionType::String)
							.required(true);
						opt
					}
				],
			},
			CommandInfo {
				name: "opt-out",
				short_desc: "Opt out of highlighting",
				long_desc: indoc!("
	                Use `/opt-out` to opt out of highlighting functionality.

	                When you opt out, your keywords and other preferences will \
	                be deleted, and others will no longer be notified of any \
	                of your messages.

	                You may opt back in later with `/opt-in`, but any \
	                information deleted when opting out cannot be restored.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "opt-in",
				short_desc: "Opt in to highlighting after having opted out",
				long_desc: indoc!("
	                Use `/opt-in` to opt in after having opted out.

	                This command has no effect if you haven't opted out using \
	                `/opt-out`.

					See `/help opt-out` for more information.",
				),
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "help",
				short_desc: "Show this help message or help for a specific command",
				long_desc: indoc!("
					Use `/help` to see a list of commands and short descriptions.
					Use `/help [command]` to see additional information about \
					the specified command.
					Use `/about` to see information about this bot.",
				),
				examples: Some(indoc!("
					Display the list of commands:
					/help

					Display the help for the `add` command:
					/help `command:` add",
				)),
				options: vec![],
			},
			CommandInfo {
				name: "ping",
				short_desc: "Show the bot's ping",
				long_desc: "Show the bot's ping, including current API, command, and database latency.",
				examples: None,
				options: vec![],
			},
			CommandInfo {
				name: "about",
				short_desc: "Show some information about this bot, including an invite link",
				long_desc:
					"Show some information about this bot, \
					like its version, source code link, and an invite link.",
				examples: None,
				options: vec![],
			},
		];

		let help_options = {
			let mut opt = Option::default();
			opt.name("command")
				.description("Command to view help for")
				.kind(CommandOptionType::String);

			for command in &commands {
				opt.add_string_choice(&command.name, &command.name);
			}

			opt
		};

		commands[commands.len() - 3].options.push(help_options);

		commands
	});
