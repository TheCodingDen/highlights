// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for sent command responses.

use serenity::{
	client::Context,
	model::id::{ChannelId, MessageId},
	prelude::TypeMapKey,
	Client,
};

use std::collections::HashMap;

/// Type key for storing a map of command messages to responses
pub struct CommandResponseMap;

impl TypeMapKey for CommandResponseMap {
	type Value = HashMap<MessageId, MessageId>;
}

/// Sets up storage for recording command responses.
pub async fn init(client: &Client) {
	let mut data = client.data.write().await;

	data.insert::<CommandResponseMap>(HashMap::new());
}

/// Records a command response so it can be deleted if the original message is deleted.
pub async fn insert_command_response(
	ctx: &Context,
	original_message: MessageId,
	response_message: MessageId,
) {
	let mut data = ctx.data.write().await;
	let map = data
		.get_mut::<CommandResponseMap>()
		.expect("Command response map not present");

	map.insert(original_message, response_message);
}

/// Deletes responses to deleted commands.
///
/// This function both deletes the message from Discord and removes it from the cache of responses.
pub async fn delete_command_response(
	ctx: &Context,
	channel: ChannelId,
	message: MessageId,
) {
	let mut data = ctx.data.write().await;
	let map = data
		.get_mut::<CommandResponseMap>()
		.expect("Command response map not preesent");

	if let Some(response) = map.remove(&message) {
		drop(data);
		if let Err(e) = channel.delete_message(ctx, response).await {
			log::error!("Error deleting response to deleted command: {}", e);
		}
	}
}
