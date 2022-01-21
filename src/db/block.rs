// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for blocked users.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::UserId;

use crate::{await_db, db::connection};

use super::IdI64Ext;

/// Represents a blocked user.
#[derive(Debug, Clone)]
pub(crate) struct Block {
	/// The user who blocked them.
	pub(crate) user_id: UserId,
	/// The user who was blocked.
	pub(crate) blocked_id: UserId,
}

impl Block {
	/// Builds a `Block` from a `Row`, in this order:
	/// - `user_id`: `INTEGER`
	/// - `blocked_id`: `INTEGER`
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Self {
			user_id: UserId::from_i64(row.get(0)?),
			blocked_id: UserId::from_i64(row.get(1)?),
		})
	}

	/// Creates the DB table to store blocked users.
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS blocks (
			user_id INTEGER NOT NULL,
			blocked_id INTEGER NOT NULL,
			PRIMARY KEY (user_id, blocked_id)
			)",
			params![],
		)
		.expect("Failed to create blocks table");
	}

	/// Fetches the list of blocks a user has added from the DB.
	pub(crate) async fn user_blocks(user_id: UserId) -> Result<Vec<Self>> {
		await_db!("user blocks": |conn| {
			let mut stmt = conn.prepare(
				"SELECT user_id, blocked_id
				FROM blocks
				WHERE user_id = ?"
			)?;

			let blocks = stmt.query_map(params![user_id.into_i64()], Self::from_row)?;

			blocks.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Adds this blocked user to the DB.
	pub(crate) async fn insert(self) -> Result<()> {
		await_db!("insert block": |conn| {
			conn.execute(
				"INSERT INTO blocks (user_id, blocked_id)
				VALUES (?, ?)",
				params![
					self.user_id.into_i64(),
					self.blocked_id.into_i64()
				],
			)?;

			Ok(())
		})
	}

	/// Checks if this block exists in the DB.
	pub(crate) async fn exists(self) -> Result<bool> {
		await_db!("block exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM blocks
				WHERE user_id = ? AND blocked_id = ?",
				params![
					self.user_id.into_i64(),
					self.blocked_id.into_i64()
				],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			).map_err(Into::into)
		})
	}

	/// Deletes this blocked user from the DB (making them not blocked anymore).
	pub(crate) async fn delete(self) -> Result<()> {
		await_db!("delete block": |conn| {
			conn.execute(
				"DELETE FROM blocks
				WHERE user_id = ? AND blocked_id = ?",
				params![
					self.user_id.into_i64(),
					self.blocked_id.into_i64()
				],
			)?;

			Ok(())
		})
	}
}
