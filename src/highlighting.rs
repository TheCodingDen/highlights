use serenity::{
	builder::CreateMessage,
	client::Context,
	http::{error::ErrorResponse, HttpError},
	model::{channel::Message, id::UserId},
	Error as SerenityError,
};

use std::{convert::TryInto, ops::Range, time::Duration};

use crate::{
	db::{Ignore, Keyword},
	global::{EMBED_COLOR, NOTIFICATION_RETRIES, PATIENCE_DURATION},
	log_discord_error, regex,
	util::MD_SYMBOL_REGEX,
	Error,
};
use tokio::time::delay_for;

pub async fn should_notify_keyword(
	ctx: &Context,
	message: &Message,
	keyword: &Keyword,
	ignores: &[Ignore],
) -> Result<Option<Range<usize>>, Error> {
	let content = &*message.content;

	for ignore in ignores {
		if find_applicable_match(&ignore.phrase, content).is_some() {
			return Ok(None);
		}
	}

	let range = match find_applicable_match(&keyword.keyword, content) {
		Some(range) => range,
		None => return Ok(None),
	};

	let user_id = UserId(keyword.user_id.try_into().unwrap());
	let channel = match ctx.cache.guild_channel(message.channel_id).await {
		Some(c) => c,
		None => match ctx.http.get_channel(message.channel_id.0).await {
			Ok(c) => match c {
				serenity::model::channel::Channel::Guild(c) => c,
				_ => {
					return Err(format!(
						"Channel {} wasn't a guild channel",
						message.channel_id
					)
					.into())
				}
			},
			Err(e) => return Err(e.into()),
		},
	};

	if !channel
		.permissions_for_user(ctx, user_id)
		.await?
		.read_messages()
	{
		return Ok(None);
	}

	Ok(Some(range))
}

pub async fn notify_keyword(
	ctx: Context,
	message: Message,
	keyword: Keyword,
	ignores: Vec<Ignore>,
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
			let message = match ctx
				.http
				.get_message(message.channel_id.0, message.id.0)
				.await
			{
				Ok(m) => m,
				Err(SerenityError::Http(err)) => match &*err {
					HttpError::UnsuccessfulRequest(ErrorResponse {
						status_code,
						..
					}) if status_code.as_u16() == 404 => {
						return Ok(());
					}
					_ => return Err(SerenityError::Http(err).into()),
				},
				Err(err) => return Err(err.into()),
			};
			let keyword_range =
				match should_notify_keyword(&ctx, &message, &keyword, &ignores)
					.await?
				{
					Some(range) => range,
					None => return Ok(()),
				};

			let msg = &message.content;
			let re = &*MD_SYMBOL_REGEX;
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
				"[(Link)](https://discord.com/channels/{}/{}/{})",
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
			let channel_mention = format!("<#{}>", message.channel_id);

			let dm_channel = user_id.create_dm_channel(&ctx).await?;

			let mut message_to_send = CreateMessage::default();
			message_to_send.embed(|e| {
				e.description(formatted_content)
					.timestamp(&message.timestamp)
					.author(|a| a.name(title))
					.field("Channel", channel_mention, true)
					.field("Message", message_link, true)
					.footer(|f| {
						f.icon_url(message.author.avatar_url().unwrap_or_else(
							|| message.author.default_avatar_url(),
						))
						.text(message.author.name)
					})
					.color(EMBED_COLOR)
			});

			let mut result = Ok(());

			for _ in 0..NOTIFICATION_RETRIES {
				let mut message_to_send = message_to_send.clone();

				match dm_channel
					.send_message(&ctx, |_| &mut message_to_send)
					.await
				{
					Ok(_) => {
						result = Ok(());
						break;
					}

					Err(SerenityError::Http(err)) => match &*err {
						HttpError::UnsuccessfulRequest(ErrorResponse {
							status_code,
							..
						}) if status_code.is_server_error() => {
							result = Err(SerenityError::Http(err).into());
						}

						HttpError::UnsuccessfulRequest(ErrorResponse {
							error,
							..
						}) if error.message
							== "Cannot send messages to this user" =>
						{
							result = Ok(());
							break;
						}

						_ => Err(SerenityError::Http(err))?,
					},

					Err(err) => Err(err)?,
				}

				delay_for(Duration::from_secs(2)).await;
			}

			result
		}
		.await;

		if let Err(error) = result {
			log_discord_error!(in channel_id, by user_id, error);
		}
	}
}

fn find_applicable_match(keyword: &str, content: &str) -> Option<Range<usize>> {
	if regex!(r"\s").is_match(keyword) {
		content
			.match_indices(keyword)
			.find(|(i, phrase)| {
				if *i != 0 || i + phrase.len() < content.len() {
					let start = i.saturating_sub(1);
					let end = usize::min(i + phrase.len() + 1, content.len());
					content
						.get(start..end)
						.map(|around| {
							regex!(r"(^|\s).*(\s|$)").is_match(around)
						})
						.unwrap_or(true)
				} else {
					true
				}
			})
			.map(|(index, _)| index..index + keyword.len())
	} else if regex!(r"[^a-zA-Z0-9]").is_match(keyword) {
		let start = content.find(keyword)?;
		Some(start..start + keyword.len())
	} else {
		let mut fragments = regex!(r"[^a-zA-Z0-9]+").split(content);

		let substring =
			fragments.find(|frag| keyword.eq_ignore_ascii_case(frag))?;

		let substring_start = substring.as_ptr() as usize;
		let content_start = content.as_ptr() as usize;
		let substring_index = substring_start - content_start;

		Some(substring_index..substring_index + keyword.len())
	}
}
