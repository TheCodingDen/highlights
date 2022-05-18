// Copyright 2022 ThatsNoMoon
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
		interactions::application_command::ApplicationCommandInteraction as Command,
	},
	prelude::TypeMapKey,
	Error as SerenityError,
};
use tinyvec::TinyVec;

use std::{collections::HashMap, fmt::Write as _, ops::Range, time::Duration};

use crate::{
	bot::util::{followup_eph, user_can_read_channel},
	db::{Ignore, Keyword, Notification, UserState, UserStateKind},
	global::{EMBED_COLOR, ERROR_COLOR, NOTIFICATION_RETRIES},
	settings::settings,
};
use indoc::indoc;
use lazy_regex::regex;
use tokio::{select, time::sleep};

pub(crate) struct CachedMessages;

impl TypeMapKey for CachedMessages {
	type Value = HashMap<MessageId, String>;
}

/// Checks if the provided keyword should be highlighted anywhere in the given message.
///
/// First each [`Ignore`](Ignore) is checked to determine if it appears in the message. If any do
/// appear, then the keyword shouldn't be highlighted and `Ok(false)` is returned. Next, the keyword
/// is similarly searched for in the message content. If it is found, the permissions of the user
/// are checked to ensure they can read the message. If they can read the message, `Ok(true)`
/// is returned.
#[tracing::instrument(
	skip_all,
	fields(
		author_id = %message.author.id,
		recipient_id = %keyword.user_id,
		message_id = %message.id,
		channel_id = %message.channel_id,
	)
)]
pub(crate) async fn should_notify_keyword(
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

	let channel = match ctx.cache.guild_channel(message.channel_id) {
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
#[tracing::instrument(
	skip_all,
	fields(
		author_id = %message.author.id,
		recipient_id = %user_id,
		message_id = %message.id,
		channel_id = %message.channel_id,
		guild_id = %guild_id,
	)
)]
pub(crate) async fn notify_keywords(
	ctx: Context,
	mut message: Message,
	keywords: TinyVec<[Keyword; 2]>,
	ignores: Vec<Ignore>,
	user_id: UserId,
	guild_id: GuildId,
) {
	ctx.data
		.write()
		.await
		.get_mut::<CachedMessages>()
		.expect("No message cache")
		.insert(message.id, message.content.clone());

	let reply_or_reaction;

	let reply = message
		.channel_id
		.await_reply(&ctx)
		.author_id(user_id)
		.timeout(settings().behavior.patience);

	let reaction = message
		.channel_id
		.await_reaction(&ctx)
		.author_id(user_id)
		.timeout(settings().behavior.patience);

	select! {
		reaction = reaction => reply_or_reaction = reaction.map(|_| ()),
		reply = reply => reply_or_reaction = reply.map(|_| ()),
	}

	if reply_or_reaction.is_none() {
		tracing::debug!("Recipient did not interact within patience duration");
		let result: Result<()> = async {
			let content = match ctx
				.data
				.write()
				.await
				.get_mut::<CachedMessages>()
				.expect("No message cache")
				.remove(&message.id)
			{
				Some(m) => m,
				None => {
					tracing::debug!(
						"Original message not found in cache - deleted"
					);
					return Ok(());
				}
			};

			tracing::debug!("Original message found in cache");

			message.content = content;

			let keywords = stream::iter(keywords)
				.map(Ok::<_, anyhow::Error>) // convert to a TryStream
				.try_filter_map(|keyword| async {
					Ok(should_notify_keyword(
						&ctx,
						&message,
						&message.content.to_lowercase(),
						&keyword,
						&ignores,
					)
					.await?
					.then(|| keyword.keyword))
				})
				.try_collect::<TinyVec<[String; 2]>>()
				.await?;

			if keywords.is_empty() {
				tracing::debug!("No keywords to notify after being patient");
				return Ok(());
			}

			let message_to_send =
				build_notification_message(&ctx, &message, &keywords, guild_id)
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
			tracing::error!("{:?}", error);
		}
	}
}

async fn build_notification_message(
	ctx: &Context,
	message: &Message,
	keywords: &[String],
	guild_id: GuildId,
) -> Result<CreateMessage<'static>> {
	let embed =
		build_notification_embed(ctx, message, keywords, guild_id).await?;

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
	guild_id: GuildId,
) -> Result<EditMessage<'static>> {
	let embed =
		build_notification_embed(ctx, message, keywords, guild_id).await?;

	let mut msg = EditMessage::default();

	msg.embed(|e| {
		*e = embed;
		e
	});

	Ok(msg)
}

#[tracing::instrument(
	skip_all,
	fields(
		author_id = %message.author.id,
		message_id = %message.id,
		channel_id = %message.channel_id,
	)
)]
async fn build_notification_embed(
	ctx: &Context,
	message: &Message,
	keywords: &[String],
	guild_id: GuildId,
) -> Result<CreateEmbed> {
	let message_link = format!(
		"[(Link)](https://discord.com/channels/{}/{}/{})",
		guild_id, message.channel_id, message.id
	);

	let channel_name = ctx
		.cache
		.guild_channel_field(message.channel_id, |c| c.name.clone())
		.context("Couldn't get channel for keyword")?;
	let (guild_name, guild_icon) = ctx
		.cache
		.guild_field(guild_id, |g| (g.name.clone(), g.icon_url()))
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

#[tracing::instrument(
	skip_all,
	fields(
		recipient_id = %user_id,
		message_id = %message_id,
	)
)]
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

#[tracing::instrument(skip(ctx))]
pub(crate) async fn delete_sent_notifications(
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
			tracing::error!("{:?}", e);
		}
	}
}

#[tracing::instrument(
	skip_all,
	fields(
		author_id = %message.author.id,
		message_id = %message.id,
		channel_id = %message.channel_id,
		guild_id = %guild_id,
	)
)]
pub(crate) async fn update_sent_notifications(
	ctx: &Context,
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
			let message_to_send =
				build_notification_edit(ctx, &message, &keywords, guild_id)
					.await?;

			let dm_channel = user_id
				.create_dm_channel(ctx)
				.await
				.context("Failed to create DM channel")?;

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
			tracing::error!("Failed to update notification: {:?}", e);
		}
	}

	delete_sent_notifications(ctx, message.channel_id, message.id, &to_delete)
		.await;

	for (_, notification_message) in to_delete {
		if let Err(e) =
			Notification::delete_notification_message(notification_message)
				.await
		{
			tracing::error!("Failed to delete notification message: {:?}", e);
		}
	}
}

/// Finds a match of the keyword in the message content.
#[tracing::instrument(skip_all)]
fn keyword_matches(keyword: &str, content: &str) -> bool {
	fn overlaps_with_mention(range: Range<usize>, content: &str) -> bool {
		regex!(r"<(@!?|&|#|a?:[a-zA-Z0-9_]*:)[0-9]+>")
			.find_iter(content)
			.any(|mention| {
				let mention = mention.range();

				range.start <= mention.end && range.end >= mention.start
			})
	}

	let (whitespace, bounded, non_alpha_num) = match keyword.is_ascii() {
		true => (regex!(r"\s"U), regex!(r"^.\b.*\b.$"U), regex!(r"\W+"U)),
		false => (regex!(r"\s"), regex!(r"^.\b.*\b.$"), regex!(r"\W+")),
	};

	if whitespace.is_match(keyword) {
		// if the keyword has whitespace, only matches of whole phrases should be considered
		content
			.match_indices(keyword)
			.filter(|(i, phrase)| {
				if *i != 0 || i + phrase.len() < content.len() {
					let start = i.saturating_sub(1);
					let end = usize::min(i + phrase.len() + 1, content.len());
					content
						.get(start..end)
						.map(|around| bounded.is_match(around))
						.unwrap_or(true)
				} else {
					true
				}
			})
			.map(|(index, _)| index..index + keyword.len())
			.any(|range| !overlaps_with_mention(range, content))
	} else if non_alpha_num.is_match(keyword) {
		// if the keyword contains non-alphanumeric characters, it could appear anywhere
		content
			.match_indices(keyword)
			.map(|(i, _)| i..i + keyword.len())
			.any(|range| !overlaps_with_mention(range, content))
	} else {
		// otherwise, it is only alphanumeric and could appear between non-alphanumeric text
		non_alpha_num
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
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn check_notify_user_state(
	ctx: &Context,
	command: &Command,
) -> Result<()> {
	let user_state = match UserState::user_state(command.user.id).await? {
		Some(user_state) => user_state,
		None => return Ok(()),
	};

	warn_for_failed_dm(ctx, command).await?;

	user_state.delete().await?;

	Ok(())
}

#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn warn_for_failed_dm(
	ctx: &Context,
	command: &Command,
) -> Result<()> {
	followup_eph(
		ctx,
		command,
		indoc!(
			"
			⚠️ I failed to DM you to notify you of your last highlighted \
			keyword. Make sure you have DMs enabled in at least one server \
			that we share."
		),
	)
	.await
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
