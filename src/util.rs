// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use serenity::{
	client::Context,
	model::{channel::Message, id::UserId},
};

use crate::{
	db::Keyword,
	global::{EMBED_COLOR, PATIENCE_DURATION},
	Error,
};
use std::{convert::TryInto, fmt::Display, ops::Range};

#[macro_export]
macro_rules! log_discord_error {
	(in $channel_id:expr, by $user_id:expr, $error:expr) => {
		log::error!(
			"Error in <#{0}> ({0}) by <@{1}> ({1}): {2}\n{2:?}",
			$channel_id,
			$user_id,
			$error
		);
	};
}

macro_rules! regex {
	($re:literal $(,)?) => {{
		static RE: once_cell::sync::OnceCell<regex::Regex> =
			once_cell::sync::OnceCell::new();
		RE.get_or_init(|| regex::Regex::new($re).unwrap())
		}};
}

pub async fn notify_keyword(
	ctx: Context,
	message: Message,
	keyword_range: Range<usize>,
	keyword: Keyword,
) {
	let user_id = UserId(keyword.user_id.try_into().unwrap());
	let channel_id = message.channel_id;
	let guild_id = message.guild_id.unwrap();

	let new_message = message
		.channel_id
		.await_reply(&ctx)
		.author_id(user_id)
		.timeout(PATIENCE_DURATION);
	if new_message.await.is_none() {
		let result: Result<(), Error> = async {
			let re = regex!(r"[_*()\[\]~`]");
			let msg = &message.content;
			let formatted_content = format!(
				"{}__**{}**__{}",
				re.replace_all(&msg[..keyword_range.start], r"\$0"),
				re.replace_all(
					&msg[keyword_range.start..keyword_range.end],
					r"\$0"
				),
				re.replace_all(&msg[keyword_range.end..], r"\$0")
			);

			let message_link = format!(
				"https://discord.com/channels/{}/{}/{}",
				guild_id, channel_id, message.id
			);

			let channel_name = ctx
				.cache
				.guild_channel_field(channel_id, |c| c.name.clone())
				.await
				.ok_or("Couldn't get channel for keyword")?;
			let guild_name = ctx
				.cache
				.guild_field(guild_id, |g| g.name.clone())
				.await
				.ok_or("Couldn't get guild for keyword")?;
			let title = format!(
				"Keyword \"{}\" seen in #{} ({})",
				keyword.keyword, channel_name, guild_name
			);

			let dm_channel = user_id.create_dm_channel(&ctx).await?;
			dm_channel
				.send_message(&ctx, |m| {
					m.embed(|e| {
						e.description(formatted_content)
							.timestamp(&message.timestamp)
							.author(|a| a.name(title).url(message_link))
							.footer(|f| {
								f.icon_url(
									message.author.avatar_url().unwrap_or_else(
										|| message.author.default_avatar_url(),
									),
								)
								.text(message.author.name)
							})
							.color(EMBED_COLOR)
					})
				})
				.await?;

			Ok(())
		}
		.await;

		if let Err(error) = result {
			log_discord_error!(in channel_id, by user_id, error);
		}
	}
}

pub async fn success(ctx: &Context, message: &Message) -> Result<(), Error> {
	message.react(ctx, '✅').await?;

	Ok(())
}

pub async fn question(ctx: &Context, message: &Message) -> Result<(), Error> {
	message.react(ctx, '❓').await?;

	Ok(())
}

pub async fn error<S: Display>(
	ctx: &Context,
	message: &Message,
	response: S,
) -> Result<(), Error> {
	let _ = message.react(ctx, '❌').await;

	message.channel_id.say(ctx, response).await?;

	Ok(())
}
