use serenity::{
	client::Context,
	model::channel::Message,
	model::id::{ChannelId, UserId},
};

use crate::{log_channel_id, Error};
use std::fmt::Display;

pub async fn report_error<E: Display>(
	ctx: &Context,
	channel_id: ChannelId,
	user_id: UserId,
	error: E,
) {
	let _ = log_channel_id()
		.say(
			&ctx,
			format!("Error in {} by {}: {}", channel_id, user_id, error),
		)
		.await;

	log::error!("Error in {} by {}: {}", channel_id, user_id, error);
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
