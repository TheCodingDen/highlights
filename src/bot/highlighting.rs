// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Functions for sending, editing, and deleting notifications.

use anyhow::{anyhow, Context as _, Result};
use futures_util::{stream, StreamExt, TryStreamExt};
use serenity::{
	builder::{CreateEmbed, CreateMessage, EditMessage},
	client::Context,
	http::{error::ErrorResponse, HttpError},
	model::{
		channel::Message,
		id::{ChannelId, GuildId, MessageId, UserId},
	},
	Error as SerenityError,
};
use tinyvec::TinyVec;

use std::{collections::HashMap, fmt::Write as _, ops::Range, time::Duration};

use crate::{
	bot::util::{optional_result, user_can_read_channel},
	db::{Ignore, Keyword, Notification, UserState, UserStateKind},
	global::{EMBED_COLOR, ERROR_COLOR, NOTIFICATION_RETRIES},
	log_discord_error, regex,
	settings::settings,
};
use indoc::indoc;
use tokio::{select, time::sleep};

/// Checks if the provided keyword should be highlighted anywhere in the given message.
///
/// First each [`Ignore`](Ignore) is checked to determine if it appears in the message. If any do
/// appear, then the keyword shouldn't be highlighted and `Ok(false)` is returned. Next, the keyword
/// is similarly searched for in the message content. If it is found, the permissions of the user
/// are checked to ensure they can read the message. If they can read the message, `Ok(true)`
/// is returned.
pub async fn should_notify_keyword(
	ctx: &Context,
	message: &Message,
	content: &str,
	keyword: &Keyword,
	ignores: &[Ignore],
) -> Result<bool> {
	if message
		.mentions
		.iter()
		.any(|mention| mention.id == keyword.user_id)
	{
		return Ok(false);
	}

	for ignore in ignores {
		if keyword_matches(&ignore.phrase, content) {
			return Ok(false);
		}
	}

	if !keyword_matches(&keyword.keyword, content) {
		return Ok(false);
	}

	let channel = match ctx.cache.guild_channel(message.channel_id).await {
		Some(c) => c,
		None => match ctx.http.get_channel(message.channel_id.0).await? {
			serenity::model::channel::Channel::Guild(c) => c,
			_ => {
				return Err(anyhow!(
					"Channel {} wasn't a guild channel",
					message.channel_id
				))
			}
		},
	};

	match user_can_read_channel(ctx, &channel, keyword.user_id).await {
		Ok(Some(true)) => Ok(true),
		Ok(Some(false)) | Ok(None) => Ok(false),
		Err(e) => Err(e).context("Failed to check permissions"),
	}
}

/// Sends a notification about a highlighted keyword.
///
/// This will first wait for the configured patience duration for a message or reaction from the
/// user of the keyword. If they don't send a message or reaction in that time, then an embed is
/// created to notify them and sent in a DM channel.
///
/// If sending the notification fails because of an internal server error, it is retried up to five
/// times with a delay of two seconds.
///
/// If sending the notification fails with `"Cannot send messages to this user"`, a corresponding
/// [`UserState`](UserState) is created.
///
/// Any other errors are logged as normal.
pub async fn notify_keywords(
	ctx: Context,
	message: Message,
	keywords: TinyVec<[Keyword; 2]>,
	ignores: Vec<Ignore>,
	user_id: UserId,
	guild_id: GuildId,
) {
	let channel_id = message.channel_id;

	let reply_or_reaction;

	let reply = message
		.channel_id
		.await_reply(&ctx)
		.author_id(user_id)
		.timeout(settings().behavior.patience);

	let reaction = message.channel_id.await_reaction(&ctx).author_id(user_id);

	select! {
		reaction = reaction => reply_or_reaction = reaction.map(|_| ()),
		reply = reply => reply_or_reaction = reply.map(|_| ()),
	}

	if reply_or_reaction.is_none() {
		let result: Result<()> = async {
			let message = match optional_result(
				ctx.http
					.get_message(message.channel_id.0, message.id.0)
					.await,
			)
			.context("Failed to fetch original messsage")?
			{
				Some(m) => m,
				None => return Ok(()),
			};

			let keywords = {
				let ctx = &ctx;
				let message = &message;
				let lowercase_content = &message.content.to_lowercase();
				let ignores = &ignores;
				stream::iter(keywords)
					.map(Ok::<_, anyhow::Error>) // convert to a TryStream
					.try_filter_map(|keyword| async move {
						Ok(should_notify_keyword(
							ctx,
							message,
							lowercase_content,
							&keyword,
							ignores,
						)
						.await?
						.then(|| keyword.keyword))
					})
					.try_collect::<TinyVec<[String; 2]>>()
					.await?
			};

			if keywords.is_empty() {
				return Ok(());
			}

			let message_to_send = build_notification_message(
				&ctx, &message, &keywords, channel_id, guild_id,
			)
			.await?;

			send_notification_message(
				&ctx,
				user_id,
				message.id,
				message_to_send,
				keywords,
			)
			.await
		}
		.await;

		if let Err(error) = result {
			log_discord_error!(in channel_id, by user_id, error);
		}
	}
}

async fn build_notification_message(
	ctx: &Context,
	message: &Message,
	keywords: &[String],
	channel_id: ChannelId,
	guild_id: GuildId,
) -> Result<CreateMessage<'static>> {
	let embed =
		build_notification_embed(ctx, message, keywords, channel_id, guild_id)
			.await?;

	let mut msg = CreateMessage::default();

	msg.embed(|e| {
		*e = embed;
		e
	});

	Ok(msg)
}

async fn build_notification_edit(
	ctx: &Context,
	message: &Message,
	keywords: &[String],
	channel_id: ChannelId,
	guild_id: GuildId,
) -> Result<EditMessage> {
	let embed =
		build_notification_embed(ctx, message, keywords, channel_id, guild_id)
			.await?;

	let mut msg = EditMessage::default();

	msg.embed(|e| {
		*e = embed;
		e
	});

	Ok(msg)
}

async fn build_notification_embed(
	ctx: &Context,
	message: &Message,
	keywords: &[String],
	channel_id: ChannelId,
	guild_id: GuildId,
) -> Result<CreateEmbed> {
	let message_link = format!(
		"[(Link)](https://discord.com/channels/{}/{}/{})",
		guild_id, channel_id, message.id
	);

	let channel_name = ctx
		.cache
		.guild_channel_field(channel_id, |c| c.name.clone())
		.await
		.context("Couldn't get channel for keyword")?;
	let (guild_name, guild_icon) = ctx
		.cache
		.guild_field(guild_id, |g| (g.name.clone(), g.icon_url()))
		.await
		.context("Couldn't get guild for keyword")?;
	let title = if keywords.len() == 1 {
		format!(
			"Keyword \"{}\" seen in #{} ({})",
			keywords[0], channel_name, guild_name
		)
	} else {
		let mut iter = keywords.iter();
		let first = iter.next().unwrap();
		let mut title =
			iter.fold(format!("Keywords \"{}\"", first), |mut s, keyword| {
				write!(s, ", \"{}\"", keyword).unwrap();
				s
			});

		write!(title, " seen in #{} ({})", channel_name, guild_name).unwrap();
		title
	};
	let channel_mention = format!("<#{}>", message.channel_id);

	let mut embed = CreateEmbed::default();

	embed
		.description(&message.content)
		.timestamp(&message.timestamp)
		.author(|a| {
			a.name(title);
			if let Some(url) = guild_icon {
				a.icon_url(url);
			}
			a
		})
		.field("Channel", channel_mention, true)
		.field("Message", message_link, true)
		.footer(|f| {
			f.icon_url(
				message
					.author
					.avatar_url()
					.unwrap_or_else(|| message.author.default_avatar_url()),
			)
			.text(&message.author.name)
		})
		.color(EMBED_COLOR);

	Ok(embed)
}

async fn send_notification_message(
	ctx: &Context,
	user_id: UserId,
	message_id: MessageId,
	message_to_send: CreateMessage<'static>,
	keywords: TinyVec<[String; 2]>,
) -> Result<()> {
	let dm_channel = user_id
		.create_dm_channel(&ctx)
		.await
		.context("Failed to create DM channel to notify user")?;

	let mut result = Ok(());

	for _ in 0..NOTIFICATION_RETRIES {
		let mut message_to_send = message_to_send.clone();

		match dm_channel
			.send_message(&ctx, |_| &mut message_to_send)
			.await
		{
			Ok(sent_message) => {
				result = Ok(());
				UserState::clear(user_id).await?;
				for keyword in keywords {
					let notification = Notification {
						original_message: message_id,
						notification_message: sent_message.id,
						keyword,
						user_id,
					};
					notification.insert().await?;
				}
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
					error, ..
				}) if error.message == "Cannot send messages to this user" => {
					let user_state = UserState {
						user_id,
						state: UserStateKind::CannotDm,
					};

					user_state.set().await?;

					result = Ok(());
					break;
				}

				_ => return Err(SerenityError::Http(err).into()),
			},

			Err(err) => {
				Err(err).context("Failed to send notification message")?
			}
		}

		sleep(Duration::from_secs(2)).await;
	}

	result
}

pub async fn delete_sent_notifications(
	ctx: &Context,
	channel_id: ChannelId,
	original_message: MessageId,
	notification_messages: &[(UserId, MessageId)],
) {
	for (user_id, message_id) in notification_messages {
		let result: Result<()> = async {
			let dm_channel = user_id.create_dm_channel(ctx).await?;

			dm_channel
				.edit_message(ctx, message_id, |m| {
					m.embed(|e| {
						e.description("*Original message deleted*")
							.color(ERROR_COLOR)
					})
				})
				.await
				.context("Failed to edit notification message")?;

			Ok(())
		}
		.await;

		if let Err(e) = result {
			log_discord_error!(in channel_id, deleted original_message, e);
		}
	}
}

pub async fn update_sent_notifications(
	ctx: &Context,
	channel_id: ChannelId,
	guild_id: GuildId,
	message: Message,
	notifications: Vec<Notification>,
) {
	let mut to_delete = vec![];

	let lowercase_content = message.content.to_lowercase();

	let notifications_by_message = notifications.into_iter().fold(
		HashMap::new(),
		|mut map, notification| {
			map.entry(notification.notification_message)
				.or_insert_with(|| {
					(notification.user_id, TinyVec::<[String; 2]>::new())
				})
				.1
				.push(notification.keyword);
			map
		},
	);

	for (message_id, (user_id, keywords)) in notifications_by_message {
		let keywords = keywords
			.into_iter()
			.filter(|keyword| keyword_matches(keyword, &lowercase_content))
			.collect::<TinyVec<[String; 2]>>();

		if keywords.is_empty() {
			to_delete.push((user_id, message_id));
			continue;
		}

		let result: Result<()> = async {
			let message_to_send = build_notification_edit(
				ctx, &message, &keywords, channel_id, guild_id,
			)
			.await?;

			let dm_channel = user_id.create_dm_channel(ctx).await.context(
				"Failed to create DM channel to update notifications",
			)?;

			dm_channel
				.edit_message(ctx, message_id, |m| {
					*m = message_to_send;
					m
				})
				.await?;

			Ok(())
		}
		.await;

		if let Err(e) = result {
			log_discord_error!(in channel_id, edited message.id, e);
		}
	}

	delete_sent_notifications(ctx, channel_id, message.id, &to_delete).await;

	for (_, notification_message) in to_delete {
		if let Err(e) =
			Notification::delete_notification_message(notification_message)
				.await
		{
			log_discord_error!(in channel_id, edited message.id, e);
		}
	}
}

/// Finds a match of the keyword in the message content.
fn keyword_matches(keyword: &str, content: &str) -> bool {
	fn overlaps_with_mention(range: Range<usize>, content: &str) -> bool {
		regex!(r"<(@!?|&|#|a?:[a-zA-Z0-9_]*:)[0-9]+>")
			.find_iter(content)
			.any(|mention| {
				let mention = mention.range();

				range.start <= mention.end && range.end >= mention.start
			})
	}

	if regex!(r"\s").is_match(keyword) {
		// if the keyword has a space, only matches of whole phrases should be considered
		content
			.match_indices(keyword)
			.filter(|(i, phrase)| {
				if *i != 0 || i + phrase.len() < content.len() {
					let start = i.saturating_sub(1);
					let end = usize::min(i + phrase.len() + 1, content.len());
					content
						.get(start..end)
						.map(|around| regex!(r"^.\b.*\b.$").is_match(around))
						.unwrap_or(true)
				} else {
					true
				}
			})
			.map(|(index, _)| index..index + keyword.len())
			.any(|range| !overlaps_with_mention(range, content))
	} else if regex!(r"\W").is_match(keyword) {
		// if the keyword contains non-alphanumeric characters, it could appear anywhere
		content
			.match_indices(keyword)
			.map(|(i, _)| i..i + keyword.len())
			.any(|range| !overlaps_with_mention(range, content))
	} else {
		// otherwise, it is only alphanumeric and could appear between non-alphanumeric text
		regex!(r"\W+")
			.split(content)
			.filter(|&frag| keyword == frag)
			.map(|substring| {
				let substring_start = substring.as_ptr() as usize;
				let content_start = content.as_ptr() as usize;
				let substring_index = substring_start - content_start;

				substring_index..substring_index + keyword.len()
			})
			.any(|range| !overlaps_with_mention(range, content))
	}
}

/// Checks the state of the last notification of the user.
///
/// If the last notification failed, send a message warning the user they should enable DMs. Clears
/// the user state afterwards.
pub async fn check_notify_user_state(
	ctx: &Context,
	message: &Message,
) -> Result<()> {
	let user_id = message.author.id;

	let user_state = match UserState::user_state(user_id).await? {
		Some(user_state) => user_state,
		None => return Ok(()),
	};

	message
		.reply(
			ctx,
			indoc!(
				"
					⚠️ I failed to DM you to notify you of your last \
					highlighted keyword. Make sure you have DMs enabled in at \
					least one server that we share."
			),
		)
		.await?;

	user_state.delete().await?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn keyword_match_basic() {
		assert!(keyword_matches("bar", "foo bar baz"));
	}

	#[test]
	fn keyword_match_phrase() {
		assert!(keyword_matches("foo bar", "baz foo bar."));
	}

	#[test]
	fn keyword_match_complex() {
		assert!(keyword_matches("$bar", "foo$bar%baz"));
	}

	#[test]
	fn keyword_match_unicode() {
		assert!(keyword_matches("ဥပမာ", "စမ်းသပ်မှု—ဥပမာ—ကျေးဇူးပြု၍ လျစ်လျူရှုပါ"));

		assert!(!keyword_matches("ဥပမာ", "စမ်းသပ်မှုဥပမာ"));
	}
}
