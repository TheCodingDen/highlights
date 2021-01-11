// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Handling for blocked users.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::UserId;

use std::convert::TryInto;

use crate::{await_db, db::connection};

/// Represents a blocked user.
#[derive(Debug, Clone)]
pub struct Block {
	/// The user who blocked them.
	pub user_id: i64,
	/// The user who was blocked.
	pub blocked_id: i64,
}

impl Block {
	/// Builds a `Block` from a `Row`, in this order:
	/// - `user_id`: `INTEGER`
	/// - `blocked_id`: `INTEGER`
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Self {
			user_id: row.get(0)?,
			blocked_id: row.get(1)?,
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
	pub async fn user_blocks(user_id: UserId) -> Result<Vec<Self>> {
		await_db!("user blocks": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT user_id, blocked_id
				FROM blocks
				WHERE user_id = ?"
			)?;

			let blocks = stmt.query_map(params![user_id], Self::from_row)?;

			blocks.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Adds this blocked user to the DB.
	pub async fn insert(self) -> Result<()> {
		await_db!("insert block": |conn| {
			conn.execute(
				"INSERT INTO blocks (user_id, blocked_id)
				VALUES (?, ?)",
				params![self.user_id, self.blocked_id],
			)?;

			Ok(())
		})
	}

	/// Checks if this block exists in the DB.
	pub async fn exists(self) -> Result<bool> {
		await_db!("block exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM blocks
				WHERE user_id = ? AND blocked_id = ?",
				params![self.user_id, self.blocked_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			).map_err(Into::into)
		})
	}

	/// Deletes this blocked user from the DB (making them not blocked anymore).
	pub async fn delete(self) -> Result<()> {
		await_db!("delete block": |conn| {
			conn.execute(
				"DELETE FROM blocks
				WHERE user_id = ? AND blocked_id = ?",
				params![self.user_id, self.blocked_id],
			)?;

			Ok(())
		})
	}
}
