// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Commands for opting out (and in) of having messages highlighted.

use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use indoc::indoc;
use rand::{distributions::Standard, Rng};
use serenity::{
	client::Context,
	collector::ComponentInteractionCollectorBuilder,
	model::interactions::{
		application_command::ApplicationCommandInteraction as Command,
		message_component::ButtonStyle,
		InteractionApplicationCommandCallbackDataFlags as ResponseFlags,
	},
};

use crate::{
	bot::util::{respond_eph, success},
	db::OptOut,
};

/// Opt-out of being highlighted.
///
/// Usage:
/// - `/opt-out`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn opt_out(ctx: Context, command: Command) -> Result<()> {
	let opt_out = OptOut {
		user_id: command.user.id,
	};

	if opt_out.exists().await? {
		return respond_eph(&ctx, &command, "❌ You already opted out!").await;
	}

	const OPT_OUT_WARNING: &str = indoc!(
		"
		⚠️ Are you sure you want to opt out?

		All of your keywords, muted channels, blocked users, and ignored phrases \
		will be lost forever.

		You will no longer be able to receive notifications.

		Others will not receive notifications about your messages."
	);

	let nonce = rand::thread_rng()
		.sample_iter::<char, _>(Standard)
		.take(90)
		.collect::<String>();

	let confirm_id = format!("confirm{}", nonce);
	let cancel_id = format!("cancel{}", nonce);

	command
		.create_interaction_response(&ctx, |r| {
			r.interaction_response_data(|m| {
				m.flags(ResponseFlags::EPHEMERAL)
					.content(OPT_OUT_WARNING)
					.components(|c| {
						c.create_action_row(|row| {
							row.create_button(|b| {
								b.style(ButtonStyle::Danger)
									.label("Opt out")
									.custom_id(&confirm_id)
							})
							.create_button(|b| {
								b.style(ButtonStyle::Secondary)
									.label("Cancel")
									.custom_id(&cancel_id)
							})
						})
					})
			})
		})
		.await?;

	let button_press = ComponentInteractionCollectorBuilder::new(&ctx)
		.collect_limit(1)
		.author_id(command.user.id)
		.filter({
			let confirm_id = confirm_id.clone();
			let cancel_id = cancel_id.clone();
			move |interaction| {
				let id = interaction.data.custom_id.as_str();
				id == confirm_id || id == cancel_id
			}
		})
		.timeout(Duration::from_secs(10))
		.build()
		.next()
		.await;

	match button_press {
		None => {
			command
				.edit_original_interaction_response(&ctx, |r| {
					r.content("Timed out.").components(|c| c)
				})
				.await?;
		}
		Some(press) => match press.data.custom_id.as_str() {
			id if id == confirm_id => {
				let opt_out = OptOut {
					user_id: press.user.id,
				};
				opt_out.clone().delete_user_data().await?;
				opt_out.insert().await?;
				command
					.edit_original_interaction_response(&ctx, |r| {
						r.content("✅ You have been opted out")
							.components(|c| c)
					})
					.await?;
			}
			id if id == cancel_id => {
				command
					.edit_original_interaction_response(&ctx, |r| {
						r.content("✅ You have not been opted out")
							.components(|c| c)
					})
					.await?;
			}
			other => {
				return Err(anyhow::anyhow!(
					"Unknown opt-out message component ID {}",
					other
				));
			}
		},
	}

	Ok(())
}

/// Opt-in to being highlighted, after having opted out.
///
/// Usage:
/// - `/opt-in`
#[tracing::instrument(
	skip_all,
	fields(
		user_id = %command.user.id,
		channel_id = %command.channel_id,
		command = %command.data.name,
	)
)]
pub(crate) async fn opt_in(ctx: Context, command: Command) -> Result<()> {
	let opt_out = OptOut {
		user_id: command.user.id,
	};

	if !opt_out.clone().exists().await? {
		return respond_eph(&ctx, &command, "❌ You haven't opted out!").await;
	}

	opt_out.delete().await?;

	success(&ctx, &command).await
}
