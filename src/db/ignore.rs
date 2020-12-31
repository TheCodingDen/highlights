// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Handling for ignored phrases.

use rusqlite::{params, Error, Row};
use serenity::model::id::{GuildId, UserId};

use std::convert::TryInto;

use crate::{await_db, db::connection};

/// Represents an ignored phrase.
#[derive(Debug, Clone)]
pub struct Ignore {
	/// The phrase that should be ignored.
	pub phrase: String,
	/// The user that ignored this phrase.
	pub user_id: i64,
	/// The guild in which the user ignored the phrase.
	pub guild_id: i64,
}

impl Ignore {
	/// Builds an `Ignore` from a `Row`, in this order:
	/// - `phrase`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `guild id`: `INTEGER`
	fn from_row(row: &Row) -> Result<Self, Error> {
		Ok(Ignore {
			phrase: row.get(0)?,
			user_id: row.get(1)?,
			guild_id: row.get(2)?,
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
	pub async fn user_guild_ignores(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Ignore>, Error> {
		await_db!("user guild ignores": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ? AND guild_id = ?"
			)?;

			let ignores = stmt.query_map(params![user_id, guild_id], Ignore::from_row)?;

			ignores.collect()
		})
	}

	/// Fetches the list of ignored phrases of the specified user across all guilds from the DB.
	pub async fn user_ignores(user_id: UserId) -> Result<Vec<Ignore>, Error> {
		await_db!("user ignores": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ?"
			)?;

			let ignores = stmt.query_map(params![user_id], Ignore::from_row)?;

			ignores.collect()
		})
	}

	/// Checks if this ignored phrase already exists in the DB.
	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("ignore exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![&*self.phrase, self.user_id, self.guild_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
	}

	/// Adds this ignored phrase to the DB.
	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert ignore": |conn| {
			conn.execute(
				"INSERT INTO guild_ignores (phrase, user_id, guild_id)
				VALUES (?, ?, ?)",
				params![&*self.phrase, self.user_id, self.guild_id],
			)?;

			Ok(())
		})
	}

	/// Deletes this ignored phrase from the DB.
	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete ignore": |conn| {
			conn.execute(
				"DELETE FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![&*self.phrase, self.user_id, self.guild_id],
			)?;

			Ok(())
		})
	}

	/// Deletes all ignored phrases of the specified user in the specified guild from the DB.
	pub async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<usize, Error> {
		await_db!("delete ignores in guild": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();
			conn.execute(
				"DELETE FROM guild_ignores
					WHERE user_id = ? AND guild_id = ?",
				params![user_id, guild_id]
			)
		})
	}
}
