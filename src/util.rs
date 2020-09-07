// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use once_cell::sync::Lazy;
use regex::Regex;
use serenity::{client::Context, model::channel::Message};

use std::fmt::Display;

use crate::Error;

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

#[macro_export]
macro_rules! regex {
	($re:literal $(,)?) => {{
		static RE: once_cell::sync::OnceCell<regex::Regex> =
			once_cell::sync::OnceCell::new();
		RE.get_or_init(|| regex::Regex::new($re).unwrap())
		}};
}

pub static MD_SYMBOL_REGEX: Lazy<Regex, fn() -> Regex> =
	Lazy::new(|| Regex::new(r"[_*()\[\]~`]").unwrap());

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
