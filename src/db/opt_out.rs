// Copyright 2021 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Handling for user opt-outs.

use anyhow::Result;
use rusqlite::params;

use crate::{await_db, db::connection};

/// Represents an opt-out made by a user.
///
/// Users that opt-out will not have their messages highlighted.
#[derive(Debug, Clone)]
pub struct OptOut {
	/// The user that opted out.
	pub user_id: i64,
}

impl OptOut {
	/// Creates the DB table to store users who have opted out.
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS opt_outs (
			user_id INTEGER PRIMARY KEY
			)",
			params![],
		)
		.expect("Failed to create opt_outs table");
	}

	/// Checks if this opt-out already exists in the DB.
	pub async fn exists(self) -> Result<bool> {
		await_db!("opt-out exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM opt_outs
				WHERE user_id = ?",
				params![self.user_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			).map_err(Into::into)
		})
	}

	/// Adds this opt-out to the DB.
	pub async fn insert(self) -> Result<()> {
		await_db!("insert opt-out": |conn| {
			conn.execute(
				"INSERT INTO opt_outs (user_id)
				VALUES (?)",
				params![self.user_id],
			)?;

			Ok(())
		})
	}

	/// Deletes this opt-out from the DB.
	pub async fn delete(self) -> Result<()> {
		await_db!("delete opt-out": |conn| {
			conn.execute(
				"DELETE FROM opt_outs
				WHERE user_id = ?",
				params![self.user_id],
			)?;

			Ok(())
		})
	}
}
