use serenity::{
	client::Context,
	model::id::{ChannelId, UserId},
};

use crate::{log_channel_id, OWNER_ID};
use std::fmt::Display;

pub async fn get_channel_for_owner_id(ctx: &Context) -> ChannelId {
	if let Some(u) = ctx.cache.user(*OWNER_ID).await {
		return u
			.create_dm_channel(ctx)
			.await
			.expect("Failed to open DM channel")
			.id;
	}

	if let Some(c) = ctx.cache.channel(*OWNER_ID).await {
		return c.id();
	}

	if let Ok(u) = ctx.http.get_user(*OWNER_ID).await {
		return u
			.create_dm_channel(ctx)
			.await
			.expect("Failed to open DM channel")
			.id;
	}

	ctx.http
		.get_channel(*OWNER_ID)
		.await
		.expect("Failed to find channel or user with OWNER_ID")
		.id()
}

pub async fn report_error<E: Display>(
	ctx: &Context,
	channel_id: ChannelId,
	user_id: UserId,
	error: E,
) {
	let _ = log_channel_id()
		.send_message(&ctx, |m| {
			m.content(format!(
				"Error in {} by {}: {}",
				channel_id, user_id, error
			))
		})
		.await;

	log::error!("Error in {} by {}: {}", channel_id, user_id, error);
}
