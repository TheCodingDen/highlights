// Copyright 2023 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for sent notification messages.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use futures_util::TryStreamExt;
use sea_orm::{
	entity::prelude::{
		DeriveActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey,
		DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
	},
	ColumnTrait, Condition, IntoActiveModel, QueryFilter, QueryOrder,
	QuerySelect,
};
use serenity::model::id::{MessageId, UserId};

use super::{connection, DbInt, IdDbExt};
use crate::global::DISCORD_EPOCH;

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "sent_notifications")]
pub struct Model {
	pub(crate) user_id: DbInt,
	pub(crate) original_message: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) notification_message: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) keyword: String,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Represents a sent notification message.
#[derive(Debug, Clone)]
pub(crate) struct Notification {
	/// The ID of the message that caused the notification to be sent.
	pub(crate) original_message: MessageId,
	/// The ID of the sent notification message.
	pub(crate) notification_message: MessageId,
	/// The keyword in the original message that caused the notification to be
	/// sent.
	pub(crate) keyword: String,
	/// The ID of the user that the notification was sent to.
	pub(crate) user_id: UserId,
}

impl Notification {
	/// Fetches the notifications that were sent because of the given message
	/// from the DB.
	#[tracing::instrument]
	pub(crate) async fn notifications_of_message(
		message_id: MessageId,
	) -> Result<Vec<Self>> {
		Entity::find()
			.filter(Column::OriginalMessage.eq(message_id.into_db()))
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Notification::from)
			.try_collect()
			.await
	}

	/// Inserts this notification into the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.original_message = %self.original_message,
			self.notification_message = %self.notification_message,
	))]
	pub(crate) async fn insert(self) -> Result<()> {
		Entity::insert(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Removes notifications in the given message from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete_notification_message(
		message_id: MessageId,
	) -> Result<()> {
		Entity::delete_many()
			.filter(Column::NotificationMessage.eq(message_id.into_db()))
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Removes all notifications sent because of the given message from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete_notifications_of_message(
		message_id: MessageId,
	) -> Result<()> {
		Entity::delete_many()
			.filter(Column::OriginalMessage.eq(message_id.into_db()))
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Gets notifications older than a certain duration from the DB.
	#[tracing::instrument]
	pub(crate) async fn notifications_before(
		count: u64,
		time: SystemTime,
	) -> Result<Vec<Notification>> {
		Entity::find()
			.filter(Column::OriginalMessage.lte(time_to_max_snowflake(time)?))
			.order_by_asc(Column::OriginalMessage)
			.limit(count)
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Notification::from)
			.try_collect()
			.await
	}

	/// Deletes a list of notifications from the DB.
	#[tracing::instrument(skip_all)]
	pub(crate) async fn delete_notifications(
		message_ids: impl IntoIterator<Item = MessageId>,
	) -> Result<()> {
		Entity::delete_many()
			.filter(message_ids.into_iter().fold(
				Condition::any(),
				|cond, id| {
					cond.add(Column::NotificationMessage.eq(id.into_db()))
				},
			))
			.exec(connection())
			.await?;

		Ok(())
	}
}

fn time_to_min_snowflake(time: SystemTime) -> Result<u64> {
	let unix = time.duration_since(UNIX_EPOCH)?.as_millis() as u64;
	let oldest_discord = unix - DISCORD_EPOCH;
	Ok(oldest_discord << 22)
}

fn time_to_max_snowflake(time: SystemTime) -> Result<u64> {
	let min = time_to_min_snowflake(time)?;
	Ok(min | (!0 >> 22))
}

impl From<Model> for Notification {
	fn from(model: Model) -> Self {
		Self {
			user_id: UserId::from_db(model.user_id),
			original_message: MessageId::from_db(model.original_message),
			notification_message: MessageId::from_db(
				model.notification_message,
			),
			keyword: model.keyword,
		}
	}
}

impl From<Notification> for Model {
	fn from(notification: Notification) -> Self {
		Self {
			user_id: notification.user_id.into_db(),
			original_message: notification.original_message.into_db(),
			notification_message: notification.notification_message.into_db(),
			keyword: notification.keyword,
		}
	}
}
