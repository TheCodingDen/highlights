// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Handling for sent notification messages.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::MessageId;

use std::convert::TryInto;

use crate::{await_db, db::connection};

/// Represents a sent notification message.
#[derive(Debug, Clone)]
pub struct Notification {
	/// The ID of the message that caused the notification to be sent.
	pub original_message: i64,
	/// The ID of the sent notification message.
	pub notification_message: i64,
	/// The keyword in the original message that caused the notification to be sent.
	pub keyword: String,
	/// The ID of the user that the notification was sent to.
	pub user_id: i64,
}

impl Notification {
	/// Builds a `Notification` from a `Row`, in this order:
	/// - `original_message`: `INTEGER`
	/// - `notification_message`: `INTEGER`
	/// - `keyword`: `TEXT`
	/// - `user_id`: `INTEGER`
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Self {
			original_message: row.get(0)?,
			notification_message: row.get(1)?,
			keyword: row.get(2)?,
			user_id: row.get(3)?,
		})
	}

	/// Creates the DB table for storing mutes.
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS sent_notifications (
			original_message INTEGER NOT NULL,
			notification_message INTEGER NOT NULL,
			keyword TEXT NOT NULL,
			user_id INTEGER NOT NULL
			)",
			params![],
		)
		.expect("Failed to create sent_notifications table");
	}

	/// Fetches the notifications that were sent because of the given message from the DB.
	pub async fn notifications_of_message(
		message_id: MessageId,
	) -> Result<Vec<Self>> {
		await_db!("notifications from message": |conn| {
			let message_id: i64 = message_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT original_message, notification_message, keyword, user_id
				FROM sent_notifications
				WHERE original_message = ?"
			)?;

			let notifications = stmt.query_map(params![message_id], Self::from_row)?;

			notifications.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Inserts this notification into the DB.
	pub async fn insert(self) -> Result<()> {
		await_db!("insert notification": |conn| {
			conn.execute(
				"INSERT INTO sent_notifications (original_message, notification_message, keyword, user_id)
				VALUES (?, ?, ?, ?)",
				params![self.original_message, self.notification_message, &*self.keyword, self.user_id],
			)?;

			Ok(())
		})
	}

	/// Removes this notification from the DB.
	pub async fn delete(self) -> Result<()> {
		await_db!("delete notification": |conn| {
			conn.execute(
				"DELETE FROM sent_notifications
				WHERE notification_message = ?",
				params![self.notification_message],
			)?;

			Ok(())
		})
	}

	/// Removes all notifications sent because of the given message from the DB.
	pub async fn delete_notifications_of_message(
		message_id: MessageId,
	) -> Result<()> {
		await_db!("delete notifications": |conn| {
			let message_id: i64 = message_id.0.try_into().unwrap();

			conn.execute(
				"DELETE FROM sent_notifications
				WHERE original_message = ?",
				params![message_id],
			)?;

			Ok(())
		})
	}
}
