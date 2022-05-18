// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for user states; whether or not the last notification DM was
//! successful.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};
use serenity::model::id::UserId;

use super::IdI64Ext;
use crate::{await_db, db::connection};

/// Description of a user's state.
#[derive(Debug, Clone)]
pub(crate) struct UserState {
	pub(crate) user_id: UserId,
	pub(crate) state: UserStateKind,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub(crate) enum UserStateKind {
	/// Indicates that the last DM sent to notify this user failed.
	CannotDm = 0,
}

impl UserState {
	const CANNOT_DM_STATE: u8 = UserStateKind::CannotDm as u8;

	/// Builds a [`UserState`] from a [`Row`], in this order:
	/// - user_id: INTEGER
	/// - state: INTEGER
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		let user_id = UserId::from_i64(row.get(0)?);
		let state = match row.get(1)? {
			Self::CANNOT_DM_STATE => UserStateKind::CannotDm,
			other => {
				return Err(rusqlite::Error::IntegralValueOutOfRange(
					1,
					other as i64,
				));
			}
		};

		Ok(Self { user_id, state })
	}

	/// Creates DB table for storing user states
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS user_states (
			user_id INTEGER PRIMARY KEY,
			state INTEGER NOT NULL
			)",
			params![],
		)
		.expect("Failed to create user_states table");
	}

	/// Fetches the state of the user with the given ID from the DB.
	///
	/// Returns `None` if the user has no recorded state.
	#[tracing::instrument]
	pub(crate) async fn user_state(user_id: UserId) -> Result<Option<Self>> {
		await_db!("user state": |conn| {

			let mut stmt = conn.prepare(
				"SELECT user_id, state
				FROM user_states
				WHERE user_id = ?"
			)?;

			stmt
				.query_row(params![user_id.into_i64()], Self::from_row)
				.optional()
				.map_err(Into::into)
		})
	}

	/// Sets the state of the user in the DB.
	#[tracing::instrument]
	pub(crate) async fn set(self) -> Result<()> {
		await_db!("set user state": |conn| {
			conn.execute(
				"INSERT INTO user_states (user_id, state)
				VALUES (?, ?)
				ON CONFLICT (user_id)
					DO UPDATE SET state = excluded.state",
				params![self.user_id.into_i64(), self.state as u8],
			)?;

			Ok(())
		})
	}

	/// Deletes this user state from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete(self) -> Result<()> {
		await_db!("delete user state": |conn| {
			conn.execute(
				"DELETE FROM user_states
				WHERE user_id = ?",
				params![self.user_id.into_i64()],
			)?;

			Ok(())
		})
	}

	/// Clears any state of the user with the given ID.
	#[tracing::instrument]
	pub(crate) async fn clear(user_id: UserId) -> Result<()> {
		await_db!("delete user state": |conn| {
			conn.execute(
				"DELETE FROM user_states
				WHERE user_id = ?",
				params![user_id.into_i64()],
			)?;

			Ok(())
		})
	}
}
