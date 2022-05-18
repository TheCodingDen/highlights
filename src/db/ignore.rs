// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for ignored phrases.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::{GuildId, UserId};

use crate::{await_db, db::connection};

use super::IdI64Ext;

/// Represents an ignored phrase.
#[derive(Debug, Clone)]
pub(crate) struct Ignore {
	/// The phrase that should be ignored.
	pub(crate) phrase: String,
	/// The user that ignored this phrase.
	pub(crate) user_id: UserId,
	/// The guild in which the user ignored the phrase.
	pub(crate) guild_id: GuildId,
}

impl Ignore {
	/// Builds an `Ignore` from a `Row`, in this order:
	/// - `phrase`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `guild id`: `INTEGER`
	fn from_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Ignore {
			phrase: row.get(0)?,
			user_id: UserId::from_i64(row.get(1)?),
			guild_id: GuildId::from_i64(row.get(2)?),
		})
	}

	/// Creates the DB table to store ignored phrases.
	pub(super) fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS guild_ignores (
			phrase TEXT NOT NULL,
			user_id INTEGER NOT NULL,
			guild_id INTEGER NOT NULL,
			PRIMARY KEY (phrase, user_id, guild_id)
			)",
			params![],
		)
		.expect("Failed to create guild_ignores table");
	}

	/// Fetches the list of ignored phrases of the specified user in the specified guild from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_guild_ignores(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Ignore>> {
		await_db!("user guild ignores": |conn| {
			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ? AND guild_id = ?"
			)?;

			let ignores = stmt.query_map(
				params![user_id.into_i64(), guild_id.into_i64()],
				Ignore::from_row
			)?;

			ignores.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Fetches the list of ignored phrases of the specified user across all guilds from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_ignores(user_id: UserId) -> Result<Vec<Ignore>> {
		await_db!("user ignores": |conn| {
			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ?"
			)?;

			let ignores =
				stmt.query_map(params![user_id.into_i64()], Ignore::from_row)?;

			ignores.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Checks if this ignored phrase already exists in the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn exists(self) -> Result<bool> {
		await_db!("ignore exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![
					&*self.phrase,
					self.user_id.into_i64(),
					self.guild_id.into_i64()
				],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			).map_err(Into::into)
		})
	}

	/// Adds this ignored phrase to the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn insert(self) -> Result<()> {
		await_db!("insert ignore": |conn| {
			conn.execute(
				"INSERT INTO guild_ignores (phrase, user_id, guild_id)
				VALUES (?, ?, ?)",
				params![
					&*self.phrase,
					self.user_id.into_i64(),
					self.guild_id.into_i64()
				],
			)?;

			Ok(())
		})
	}

	/// Deletes this ignored phrase from the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn delete(self) -> Result<()> {
		await_db!("delete ignore": |conn| {
			conn.execute(
				"DELETE FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![
					&*self.phrase,
					self.user_id.into_i64(),
					self.guild_id.into_i64()
				],
			)?;

			Ok(())
		})
	}

	/// Deletes all ignored phrases of the specified user in the specified guild from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<usize> {
		await_db!("delete ignores in guild": |conn| {
			conn.execute(
				"DELETE FROM guild_ignores
					WHERE user_id = ? AND guild_id = ?",
				params![user_id.into_i64(), guild_id.into_i64()]
			).map_err(Into::into)
		})
	}
}
