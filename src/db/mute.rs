// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for mutes.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::{ChannelId, UserId};

use crate::{await_db, db::connection};

use super::IdI64Ext;

/// Represents a muted channel.
#[derive(Debug, Clone)]
pub(crate) struct Mute {
	/// The ID of the user who muted the channel.
	pub(crate) user_id: UserId,
	/// The ID of the channel that was muted.
	pub(crate) channel_id: ChannelId,
}

impl Mute {
	/// Builds a `Mute` from a `Row`, in this order:
	/// - user_id: INTEGER
	/// - channel_id: INTEGER
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Mute {
			user_id: UserId::from_i64(row.get(0)?),
			channel_id: ChannelId::from_i64(row.get(1)?),
		})
	}

	/// Creates the DB table for storing mutes.
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS mutes (
			user_id INTEGER NOT NULL,
			channel_id INTEGER NOT NULL,
			PRIMARY KEY (user_id, channel_id)
			)",
			params![],
		)
		.expect("Failed to create follows table");
	}

	/// Fetches a list of mutes for the user with the given ID from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_mutes(user_id: UserId) -> Result<Vec<Mute>> {
		await_db!("user mutes": |conn| {

			let mut stmt = conn.prepare(
				"SELECT user_id, channel_id
				FROM mutes
				WHERE user_id = ?"
			)?;

			let mutes =
				stmt.query_map(params![user_id.into_i64()], Mute::from_row)?;

			mutes.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Checks if this mute exists in the DB.
	///
	/// Returns true if a mute with `self.user_id` and `self.channel_id` exists in the DB.
	#[tracing::instrument]
	pub(crate) async fn exists(self) -> Result<bool> {
		await_db!("mute exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM mutes
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id.into_i64(), self.channel_id.into_i64()],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			).map_err(Into::into)
		})
	}

	/// Inserts this mute into the DB.
	#[tracing::instrument]
	pub(crate) async fn insert(self) -> Result<()> {
		await_db!("insert mute": |conn| {
			conn.execute(
				"INSERT INTO mutes (user_id, channel_id)
				VALUES (?, ?)",
				params![self.user_id.into_i64(), self.channel_id.into_i64()],
			)?;

			Ok(())
		})
	}

	/// Deletes this mute from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete(self) -> Result<()> {
		await_db!("delete mute": |conn| {
			conn.execute(
				"DELETE FROM mutes
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id.into_i64(), self.channel_id.into_i64()],
			)?;

			Ok(())
		})
	}
}
